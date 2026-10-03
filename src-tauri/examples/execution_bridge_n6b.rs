//! KSJ Nexus N6-B execution bridge for FileOrbit V0.5 (contract `fileorbit-execution/2`).
//!
//! New version of the N6-A execution bridge (examples/execution_bridge.rs, `fileorbit-execution/1`, frozen
//! and unchanged). Same executor model: the UNMODIFIED `src/lib.rs` is included and FileOrbit's own helpers
//! do all validation and filesystem work; only the execute_move_plan / undo_move_transaction orchestration
//! is mirrored (AppHandle dependency). Separate from the READ_ONLY bridge.
//!
//! Authorization is narrowed from folder level to EXACT FILE level. Execution requires ALL of:
//!   1. folder allowlist PASS (source_roots / approved_roots, unchanged from N6-A)
//!   2. exact-file approval binding PASS: the approval record (written by the Nexus/KSJ approval gate into
//!      `approvals_dir`, never taken from the request) must match the request on
//!      approval_id + exact source file + exact destination file + source_size + source_modified_ms
//!   3. FileOrbit native validation PASS (validate_move_plan, source snapshot, no overwrite, containment)
//! Any mismatch fails closed with `APPROVAL_BINDING_MISMATCH`.
//!
//! Approval consumption semantics (deterministic, tested):
//! - consumed ONLY when the operation executes successfully (`approval_used` in the ledger);
//! - a binding mismatch or FileOrbit validation failure does NOT consume the approval (recorded as
//!   `binding_rejected` / no ledger entry) because a wrong request must not be able to burn a KSJ approval,
//!   and the approval can never authorize anything other than its exact binding anyway;
//! - replay after success → `APPROVAL_ALREADY_USED`.
//! N6 bridge-added guard kept: Undo is blocked with `UNDO_TARGET_CHANGED` if the moved file no longer matches
//! the size/modified_ms recorded at Move (not a FileOrbit V0.5 RC2 feature).
#![allow(dead_code, unused_imports)]

mod fileorbit {
    include!("../src/lib.rs");

    pub mod exec {
        use super::*;
        use serde_json::{json, Value};
        use std::path::PathBuf;

        pub const CONTRACT: &str = "fileorbit-execution/2";
        pub const OPS: [&str; 3] = ["status", "safe_move", "undo"];
        const NON_HUMAN: [&str; 7] = ["claude-code", "chatgpt", "gemini", "antigravity", "codex", "runner", "nexus"];

        #[derive(Clone, Debug)]
        pub struct Config {
            pub source_roots: Vec<PathBuf>,
            pub approved_roots: Vec<PathBuf>,
            pub journal: PathBuf,
            pub approvals_dir: PathBuf,
        }

        impl Config {
            pub fn ledger(&self) -> PathBuf {
                self.journal.with_file_name("n6b-execution-ledger.jsonl")
            }
        }

        fn canon(p: &Path) -> Option<PathBuf> {
            std::fs::canonicalize(p).ok()
        }

        fn within(p: &Path, roots: &[PathBuf]) -> bool {
            let Some(c) = canon(p) else { return false };
            roots.iter().filter_map(|r| canon(r)).any(|r| c.starts_with(&r))
        }

        /// Exact file identity: canonical existing parent + file name, Windows-normalized (case, trailing dot/space).
        fn file_ident(p: &Path) -> Option<PathBuf> {
            let parent = canon(p.parent()?)?;
            Some(windows_target_identity(&parent.join(p.file_name()?)))
        }

        fn err(op: &str, code: &str, msg: &str) -> Value {
            json!({"ok": false, "contract": CONTRACT, "mode": "EXECUTION", "op": op, "error": code, "message": msg})
        }

        fn ok(op: &str, result: Value) -> Value {
            json!({"ok": true, "contract": CONTRACT, "mode": "EXECUTION",
                   "fileorbit_version": env!("CARGO_PKG_VERSION"), "op": op, "result": result})
        }

        fn mtime_ms(p: &Path) -> Option<u64> {
            std::fs::metadata(p).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
        }

        fn ledger_lines(cfg: &Config) -> Vec<Value> {
            std::fs::read_to_string(cfg.ledger()).unwrap_or_default().lines()
                .filter_map(|l| serde_json::from_str(l).ok()).collect()
        }

        fn ledger_append(cfg: &Config, v: &Value) -> Result<(), String> {
            let mut f = OpenOptions::new().create(true).append(true).open(cfg.ledger()).map_err(|e| e.to_string())?;
            writeln!(f, "{}", v).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())
        }

        /// Loads the approval record from approvals_dir (trusted gate output, not the request).
        fn approval(v: &Value, op: &str, cfg: &Config) -> Result<(String, Value), Value> {
            let id = v.get("approval_id").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || "_#.-".contains(c)) {
                return Err(err(op, "APPROVAL_REQUIRED", "valid approval_id required for mutating op"));
            }
            let file = cfg.approvals_dir.join(format!("{}.json", id.replace('#', "_")));
            let rec: Value = match std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str(&t).ok()) {
                Some(r) => r,
                None => return Err(err(op, "APPROVAL_REQUIRED", "approval record not found in approvals_dir")),
            };
            let by = rec["approved_by"].as_str().unwrap_or("");
            if rec["approval_id"].as_str() != Some(id.as_str()) || by.is_empty() || NON_HUMAN.contains(&by)
                || !rec["ops"].as_array().map(|a| a.iter().any(|x| x == op)).unwrap_or(false) {
                return Err(err(op, "APPROVAL_INVALID", "approval record id/approver/ops invalid"));
            }
            if ledger_lines(cfg).iter().any(|l| l["kind"] == "approval_used" && l["approval_id"] == id.as_str() && l["op"] == op) {
                return Err(err(op, "APPROVAL_ALREADY_USED", &id));
            }
            Ok((id, rec))
        }

        fn binding_mismatch(cfg: &Config, op: &str, id: &str, what: &[&str]) -> Value {
            let _ = ledger_append(cfg, &json!({"kind": "binding_rejected", "approval_id": id, "op": op, "mismatch": what}));
            err(op, "APPROVAL_BINDING_MISMATCH", &format!("request does not match approval binding: {:?}", what))
        }

        struct Lock(PathBuf);
        impl Drop for Lock {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        fn lock(cfg: &Config) -> Result<Lock, String> {
            let p = cfg.journal.with_file_name("n6b-execution.lock");
            OpenOptions::new().write(true).create_new(true).open(&p).map_err(|_| "execution lock busy".to_string())?;
            Ok(Lock(p))
        }

        pub fn handle(req: &str, cfg: &Config) -> Value {
            let v: Value = match serde_json::from_str(req) {
                Ok(v) => v,
                Err(e) => return err("", "BAD_REQUEST", &e.to_string()),
            };
            if v.get("contract").and_then(|c| c.as_str()) != Some(CONTRACT) {
                return err("", "BAD_REQUEST", "contract mismatch");
            }
            let op = v.get("op").and_then(|o| o.as_str()).unwrap_or("");
            match op {
                "status" => ok(op, json!({"ops": OPS, "journal": cfg.journal, "approvals_dir": cfg.approvals_dir,
                    "source_root_count": cfg.source_roots.len(), "approved_root_count": cfg.approved_roots.len(),
                    "authorization": "folder allowlist AND exact-file approval binding (approval_id, source, destination, size, modified_ms) AND FileOrbit validation",
                    "approval_consumption": "only on successful execution; binding mismatch / validation failure do not consume",
                    "bridge_added_guard": "UNDO_TARGET_CHANGED (size+modified_ms) - not a FileOrbit V0.5 RC2 feature"})),
                "safe_move" => safe_move(&v, cfg),
                "undo" => undo(&v, cfg),
                _ => err(op, "OP_NOT_ALLOWED", op),
            }
        }

        fn safe_move(v: &Value, cfg: &Config) -> Value {
            let op = "safe_move";
            let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (download_root, approved_root) = (s("source_root"), s("approved_root"));
            let Some(items) = v.get("items").and_then(|x| x.as_array()) else { return err(op, "BAD_REQUEST", "items required") };
            if items.len() != 1 {
                return err(op, "BATCH_NOT_ALLOWED", "exactly one item");
            }
            let it = &items[0];
            let item = MovePlanItem {
                source: it["source"].as_str().unwrap_or("").to_string(),
                target: it["target"].as_str().unwrap_or("").to_string(),
                expected_size: it["expected_size"].as_u64(),
                expected_modified: it["expected_modified"].as_u64(),
            };
            if item.expected_size.is_none() || item.expected_modified.is_none() {
                return err(op, "BAD_REQUEST", "expected_size and expected_modified required (source snapshot)");
            }
            // (0) approval record must exist, be human, cover this op and be unused (replay -> APPROVAL_ALREADY_USED)
            let (approval_id, rec) = match approval(v, op, cfg) { Ok(a) => a, Err(e) => return e };
            // (1) folder allowlist (security boundary, unchanged from N6-A)
            if !within(Path::new(&download_root), &cfg.source_roots) || !within(Path::new(&approved_root), &cfg.approved_roots)
                || !within(Path::new(&item.source), &cfg.source_roots)
                || !Path::new(&item.target).parent().map(|p| within(p, &cfg.approved_roots)).unwrap_or(false) {
                return err(op, "ROOT_NOT_ALLOWED", "source/destination outside execution folder allowlist");
            }
            // (2) exact-file approval binding
            let mut bad = vec![];
            if file_ident(Path::new(&item.source)).is_none()
                || file_ident(Path::new(&item.source)) != rec["source"].as_str().and_then(|x| file_ident(Path::new(x))) {
                bad.push("source");
            }
            if file_ident(Path::new(&item.target)).is_none()
                || file_ident(Path::new(&item.target)) != rec["destination"].as_str().and_then(|x| file_ident(Path::new(x))) {
                bad.push("destination");
            }
            if item.expected_size != rec["source_size"].as_u64() { bad.push("source_size") }
            if item.expected_modified != rec["source_modified_ms"].as_u64() { bad.push("source_modified_ms") }
            if !bad.is_empty() {
                return binding_mismatch(cfg, op, &approval_id, &bad);
            }
            let _guard = match lock(cfg) { Ok(g) => g, Err(e) => return err(op, "LOCKED", &e) };
            // (3) orchestration mirrored from FileOrbit execute_move_plan; FileOrbit helpers execute
            let jp = cfg.journal.clone();
            if let Err(e) = recover_truncated_journal_tail_path(&jp) { return err(op, "JOURNAL_ERROR", &e) }
            if jp.exists() { if let Err(e) = read_journal_entries_path(&jp) { return err(op, "JOURNAL_ERROR", &e) } }
            let checks = match validate_move_plan(download_root.clone(), approved_root.clone(), vec![item.clone()]) {
                Ok(c) => c,
                Err(e) => return err(op, "VALIDATION_FAILED", &e),
            };
            if let Some(b) = checks.iter().find(|x| !x.ok) {
                return err(op, "VALIDATION_FAILED", &b.reason);
            }
            let (Ok(ar), Ok(dr)) = (std::fs::canonicalize(&approved_root), std::fs::canonicalize(&download_root)) else {
                return err(op, "VALIDATION_FAILED", "root canonicalize failed");
            };
            if let Err(e) = revalidate_move_item_at_execution(&item, &dr, &ar) { return err(op, "VALIDATION_FAILED", &e) }
            let tx = new_transaction_id();
            let planned = JournalEntry { transaction_id: tx.clone(), timestamp: now_ms(), source: item.source.clone(),
                                         target: item.target.clone(), status: "planned".into(), error: None };
            if let Err(e) = append_journal_prevalidated_path(&jp, &planned) { return err(op, "JOURNAL_ERROR", &e) }
            let fail = |msg: String| -> Value {
                let _ = append_journal_prevalidated_path(&jp, &JournalEntry { status: "failed".into(), error: Some(msg.clone()), ..planned.clone() });
                err(op, "EXECUTION_FAILED", &msg)
            };
            if let Err(msg) = revalidate_move_item_at_execution(&item, &dr, &ar) { return fail(msg) }
            let tp = Path::new(&item.target);
            if let Err(e) = prepare_target_parent(tp, &ar) { return fail(e) }
            if let Err(e) = move_with_audit(Path::new(&item.source), tp,
                || append_journal_prevalidated_path(&jp, &JournalEntry { status: "moved".into(), error: None, ..planned.clone() }), "이동") {
                return fail(e);
            }
            let size = std::fs::metadata(tp).map(|m| m.len()).ok();
            let modified = mtime_ms(tp);
            let _ = ledger_append(cfg, &json!({"kind": "moved_snapshot", "transaction_id": tx, "target": item.target,
                                                "size": size, "modified_ms": modified}));
            let _ = ledger_append(cfg, &json!({"kind": "approval_used", "approval_id": approval_id, "op": op, "transaction_id": tx}));
            ok(op, json!({"transaction_id": tx, "moved": 1, "failed": 0, "source": item.source, "target": item.target,
                          "target_size": size, "target_modified_ms": modified, "journal": jp,
                          "authorization": ["folder_allowlist", "exact_file_binding", "fileorbit_validation"]}))
        }

        fn undo(v: &Value, cfg: &Config) -> Value {
            let op = "undo";
            let (approval_id, rec) = match approval(v, op, cfg) { Ok(a) => a, Err(e) => return e };
            let tx = v.get("transaction_id").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            if tx.is_empty() { return err(op, "BAD_REQUEST", "transaction_id required") }
            let _guard = match lock(cfg) { Ok(g) => g, Err(e) => return err(op, "LOCKED", &e) };
            let p = cfg.journal.clone();
            if !p.exists() { return err(op, "NOTHING_TO_UNDO", "no journal") }
            if let Err(e) = recover_truncated_journal_tail_path(&p) { return err(op, "JOURNAL_ERROR", &e) }
            let entries = match read_journal_entries_path(&p) { Ok(e) => e, Err(e) => return err(op, "JOURNAL_ERROR", &e) };
            if let Err(e) = validate_journal_transition_structure(&entries) { return err(op, "JOURNAL_ERROR", &e) }
            let latest = latest_success_indices(&entries, &tx);
            let undoable: Vec<JournalEntry> = entries.iter().enumerate()
                .filter(|(i, e)| e.transaction_id == tx && e.status == "moved" && latest.get(&e.source) == Some(i))
                .map(|(_, e)| e.clone()).collect();
            if undoable.len() != 1 { return err(op, "NOTHING_TO_UNDO", "no single undoable moved item for transaction") }
            let e = undoable[0].clone();
            let (src, tgt) = (Path::new(&e.source), Path::new(&e.target));
            if !src.parent().map(|x| within(x, &cfg.source_roots)).unwrap_or(false) || !within(tgt, &cfg.approved_roots) {
                return err(op, "ROOT_NOT_ALLOWED", "undo paths outside execution folder allowlist");
            }
            // exact-file binding for undo: approval must name this source and destination
            let mut bad = vec![];
            if file_ident(src) != rec["source"].as_str().and_then(|x| file_ident(Path::new(x))) { bad.push("source") }
            if file_ident(tgt) != rec["destination"].as_str().and_then(|x| file_ident(Path::new(x))) { bad.push("destination") }
            if !bad.is_empty() { return binding_mismatch(cfg, op, &approval_id, &bad) }
            let undo_fail = |code: &str, msg: String| -> Value {
                let _ = append_journal_prevalidated_path(&p, &JournalEntry { status: "undo_failed".into(), timestamp: now_ms(), error: Some(msg.clone()), ..e.clone() });
                err(op, code, &msg)
            };
            let snap = ledger_lines(cfg).into_iter().rev()
                .find(|l| l["kind"] == "moved_snapshot" && l["transaction_id"] == tx.as_str() && l["target"] == e.target.as_str());
            let Some(snap) = snap else { return undo_fail("UNDO_TARGET_CHANGED", "no moved snapshot recorded - cannot verify current state".into()) };
            if !source_snapshot_matches(tgt, snap["size"].as_u64(), snap["modified_ms"].as_u64()) {
                return undo_fail("UNDO_TARGET_CHANGED", "Undo 중단: 이동된 파일이 Move 이후 변경됨 (size/modified_ms 불일치)".into());
            }
            if let Err(msg) = undo_precheck(src, tgt) { return undo_fail("UNDO_PRECHECK_FAILED", msg) }
            if let Err(x) = move_with_audit(tgt, src,
                || append_journal_prevalidated_path(&p, &JournalEntry { status: "undone".into(), timestamp: now_ms(), error: None, ..e.clone() }), "Undo") {
                return undo_fail("UNDO_FAILED", x);
            }
            let _ = ledger_append(cfg, &json!({"kind": "approval_used", "approval_id": approval_id, "op": op, "transaction_id": tx}));
            ok(op, json!({"transaction_id": tx, "undone": 1, "restored": e.source, "from": e.target}))
        }
    }
}

use fileorbit::exec::{self, Config};
use std::io::Read;
use std::path::PathBuf;

fn config() -> Option<Config> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("fileorbit-execution-config.json")).ok()?).ok()?;
    let list = |k: &str| -> Vec<PathBuf> {
        v.get(k).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(PathBuf::from)).collect()).unwrap_or_default()
    };
    Some(Config { source_roots: list("source_roots"), approved_roots: list("approved_roots"),
                  journal: PathBuf::from(v.get("journal")?.as_str()?), approvals_dir: PathBuf::from(v.get("approvals_dir")?.as_str()?) })
}

fn main() {
    let mut req = String::new();
    let _ = std::io::stdin().read_to_string(&mut req);
    let out = match config() {
        Some(cfg) => exec::handle(&req, &cfg),
        None => serde_json::json!({"ok": false, "contract": exec::CONTRACT, "mode": "EXECUTION", "op": "",
                                   "error": "CONFIG_MISSING", "message": "fileorbit-execution-config.json (with approvals_dir) required (fail closed)"}),
    };
    println!("{}", out);
    std::process::exit(if out.get("ok").and_then(|o| o.as_bool()) == Some(true) { 0 } else { 2 });
}

#[cfg(test)]
mod tests {
    use super::fileorbit::exec::{handle, Config};
    use serde_json::{json, Value};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::UNIX_EPOCH;

    fn env(name: &str) -> (PathBuf, Config) {
        let d = std::env::temp_dir().join(format!("fo-n6b-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        for s in ["source", "destination", "journal", "approvals"] {
            fs::create_dir_all(d.join(s)).unwrap();
        }
        let cfg = Config { source_roots: vec![d.join("source")], approved_roots: vec![d.join("destination")],
                           journal: d.join("journal").join("n6b-journal.jsonl"), approvals_dir: d.join("approvals") };
        (d, cfg)
    }

    fn snap(p: &Path) -> (u64, u64) {
        let m = fs::metadata(p).unwrap();
        (m.len(), m.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64)
    }

    /// Approval record as written by the Nexus/KSJ gate (bound to one exact file pair + snapshot).
    fn approve(d: &Path, id: &str, src: &Path, dst: &Path) {
        let (s, m) = snap(src);
        fs::write(d.join("approvals").join(format!("{}.json", id.replace('#', "_"))), json!({
            "approval_id": id, "approved_by": "ksj", "ops": ["safe_move", "undo"],
            "source": src, "destination": dst, "source_size": s, "source_modified_ms": m}).to_string()).unwrap();
    }

    fn mv(d: &Path, id: &str, src: &Path, dst: &Path, size: u64, mtime: u64) -> String {
        json!({"contract": "fileorbit-execution/2", "op": "safe_move", "approval_id": id,
               "source_root": d.join("source"), "approved_root": d.join("destination"),
               "items": [{"source": src, "target": dst, "expected_size": size, "expected_modified": mtime}]}).to_string()
    }

    fn setup(name: &str) -> (PathBuf, Config, PathBuf, PathBuf, u64, u64) {
        let (d, cfg) = env(name);
        let src = d.join("source").join("doc.pdf");
        fs::write(&src, b"synthetic n6b").unwrap();
        fs::write(d.join("source").join("other.pdf"), b"other synthetic").unwrap();
        let dst = d.join("destination").join("doc.pdf");
        approve(&d, "KSJ#B1", &src, &dst);
        let (s, m) = snap(&src);
        (d, cfg, src, dst, s, m)
    }

    #[test]
    fn bind_exact_match_executes_and_replay_blocked() {
        let (d, cfg, src, dst, s, m) = setup("exact");
        let r = handle(&mv(&d, "KSJ#B1", &src, &dst, s, m), &cfg);
        assert_eq!(r["ok"], true, "{r}");
        assert!(!src.exists() && dst.exists());
        assert!(fs::read_to_string(&cfg.journal).unwrap().contains("\"moved\""));
        assert_eq!(handle(&mv(&d, "KSJ#B1", &src, &dst, s, m), &cfg)["error"], "APPROVAL_ALREADY_USED"); // 6 replay
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bind_other_source_in_same_folder_blocked() {
        let (d, cfg, _src, dst, _s, _m) = setup("othersrc");
        let other = d.join("source").join("other.pdf");
        let (os_, om) = snap(&other);
        let r = handle(&mv(&d, "KSJ#B1", &other, &dst, os_, om), &cfg);
        assert_eq!(r["error"], "APPROVAL_BINDING_MISMATCH", "{r}");
        assert!(other.exists() && !dst.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bind_same_source_other_destination_blocked() {
        let (d, cfg, src, _dst, s, m) = setup("otherdst");
        let r = handle(&mv(&d, "KSJ#B1", &src, &d.join("destination").join("renamed.pdf"), s, m), &cfg);
        assert_eq!(r["error"], "APPROVAL_BINDING_MISMATCH", "{r}");
        assert!(src.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bind_size_or_mtime_mismatch_blocked_without_consuming_approval() {
        let (d, cfg, src, dst, s, m) = setup("snapshot");
        assert_eq!(handle(&mv(&d, "KSJ#B1", &src, &dst, s + 1, m), &cfg)["error"], "APPROVAL_BINDING_MISMATCH"); // 4 size
        assert_eq!(handle(&mv(&d, "KSJ#B1", &src, &dst, s, m + 1), &cfg)["error"], "APPROVAL_BINDING_MISMATCH"); // 5 mtime
        assert!(src.exists() && !dst.exists());
        let ledger = fs::read_to_string(cfg.ledger()).unwrap();
        assert!(ledger.contains("binding_rejected") && !ledger.contains("approval_used"));
        assert_eq!(handle(&mv(&d, "KSJ#B1", &src, &dst, s, m), &cfg)["ok"], true); // mismatch did not consume approval
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bind_live_file_changed_after_approval_blocked_by_fileorbit_snapshot() {
        let (d, cfg, src, dst, s, m) = setup("live");
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&src, b"synthetic n6b changed after approval").unwrap();
        let r = handle(&mv(&d, "KSJ#B1", &src, &dst, s, m), &cfg); // request = approved values, live file differs
        assert_eq!(r["error"], "VALIDATION_FAILED", "{r}");
        assert!(src.exists() && !dst.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn bind_missing_or_ai_approval_and_folder_allowlist_blocked() {
        let (d, cfg, src, dst, s, m) = setup("approval");
        assert_eq!(handle(&mv(&d, "KSJ#NONE", &src, &dst, s, m), &cfg)["error"], "APPROVAL_REQUIRED");
        let rec: Value = json!({"approval_id": "AI#1", "approved_by": "claude-code", "ops": ["safe_move"], "source": src,
                                "destination": dst, "source_size": s, "source_modified_ms": m});
        fs::write(d.join("approvals").join("AI_1.json"), rec.to_string()).unwrap();
        assert_eq!(handle(&mv(&d, "AI#1", &src, &dst, s, m), &cfg)["error"], "APPROVAL_INVALID");
        let outside = std::env::temp_dir().join(format!("fo-n6b-outside-{}", std::process::id()));
        fs::create_dir_all(&outside).unwrap();
        let r = handle(&mv(&d, "KSJ#B1", &src, &outside.join("doc.pdf"), s, m), &cfg);
        assert_eq!(r["error"], "ROOT_NOT_ALLOWED", "{r}");
        assert!(src.exists());
        let _ = fs::remove_dir_all(outside);
        let _ = fs::remove_dir_all(d);
    }
}
