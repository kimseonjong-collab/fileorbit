//! KSJ Nexus N6-A EXECUTION bridge for FileOrbit V0.5 (contract `fileorbit-execution/1`).
//!
//! SEPARATE from the frozen READ_ONLY bridge (`readonly_bridge`, contract `fileorbit-readonly/1`).
//! - FileOrbit is the executor: the UNMODIFIED `src/lib.rs` is included and its own helpers do all
//!   validation and filesystem work (validate_move_plan, revalidate_move_item_at_execution,
//!   prepare_target_parent, move_with_audit/safe_move_file, undo_precheck, journal functions).
//! - Only the ~40-line orchestration of `execute_move_plan` / `undo_move_transaction` is repeated here
//!   because those Tauri commands need an AppHandle (GUI journal) and Tauri State (lock).
//! - Journal: a dedicated N6-A test journal from the config file, NOT the GUI app-data journal.
//! - Ops: exactly `status`, `safe_move` (one item), `undo`. Everything else `OP_NOT_ALLOWED`.
//! - Approval: every mutating op requires an approval_id (Nexus approval record); each approval_id can be
//!   used once per op (bridge ledger) -> replay `APPROVAL_ALREADY_USED`.
//! - N6 BRIDGE-ADDED SAFETY GUARD (not a FileOrbit V0.5 RC2 feature): size + modified_ms of the moved
//!   target are recorded at Move; before Undo the target must still match, else `UNDO_TARGET_CHANGED`.
#![allow(dead_code, unused_imports)]

mod fileorbit {
    include!("../src/lib.rs");

    pub mod exec {
        use super::*;
        use serde_json::{json, Value};
        use std::path::PathBuf;

        pub const CONTRACT: &str = "fileorbit-execution/1";
        pub const OPS: [&str; 3] = ["status", "safe_move", "undo"];

        #[derive(Clone, Debug)]
        pub struct Config {
            pub source_roots: Vec<PathBuf>,
            pub approved_roots: Vec<PathBuf>,
            pub journal: PathBuf,
        }

        impl Config {
            pub fn ledger(&self) -> PathBuf {
                self.journal.with_file_name("n6a-execution-ledger.jsonl")
            }
        }

        fn canon(p: &Path) -> Option<PathBuf> {
            std::fs::canonicalize(p).ok()
        }

        fn within(p: &Path, roots: &[PathBuf]) -> bool {
            let Some(c) = canon(p) else { return false };
            roots.iter().filter_map(|r| canon(r)).any(|r| c.starts_with(&r))
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

        fn approval(v: &Value, op: &str, cfg: &Config) -> Result<String, Value> {
            let id = v.get("approval_id").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            if id.is_empty() {
                return Err(err(op, "APPROVAL_REQUIRED", "approval_id required for mutating op"));
            }
            if ledger_lines(cfg).iter().any(|l| l["kind"] == "approval_used" && l["approval_id"] == id.as_str() && l["op"] == op) {
                return Err(err(op, "APPROVAL_ALREADY_USED", &id));
            }
            Ok(id)
        }

        /// Exclusive per-journal lock (stands in for Tauri State<JournalMutationLock>).
        struct Lock(PathBuf);
        impl Drop for Lock {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        fn lock(cfg: &Config) -> Result<Lock, String> {
            let p = cfg.journal.with_file_name("n6a-execution.lock");
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
                "status" => ok(op, json!({"ops": OPS, "journal": cfg.journal, "source_root_count": cfg.source_roots.len(),
                    "approved_root_count": cfg.approved_roots.len(),
                    "bridge_added_guard": "UNDO_TARGET_CHANGED (size+modified_ms) - not a FileOrbit V0.5 RC2 feature",
                    "reused": ["validate_move_plan", "revalidate_move_item_at_execution", "prepare_target_parent",
                               "move_with_audit", "safe_move_file", "undo_precheck", "journal"]})),
                "safe_move" => safe_move(&v, cfg),
                "undo" => undo(&v, cfg),
                _ => err(op, "OP_NOT_ALLOWED", op),
            }
        }

        fn safe_move(v: &Value, cfg: &Config) -> Value {
            let op = "safe_move";
            let approval_id = match approval(v, op, cfg) { Ok(a) => a, Err(e) => return e };
            let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let (download_root, approved_root) = (s("source_root"), s("approved_root"));
            let Some(items) = v.get("items").and_then(|x| x.as_array()) else { return err(op, "BAD_REQUEST", "items required") };
            if items.len() != 1 {
                return err(op, "BATCH_NOT_ALLOWED", "N6-A allows exactly one item");
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
            if !within(Path::new(&download_root), &cfg.source_roots) || !within(Path::new(&approved_root), &cfg.approved_roots) {
                return err(op, "ROOT_NOT_ALLOWED", "source_root/approved_root outside execution allowlist");
            }
            let _guard = match lock(cfg) { Ok(g) => g, Err(e) => return err(op, "LOCKED", &e) };
            // ---- orchestration mirrored from FileOrbit execute_move_plan (journal path = N6-A test journal)
            let jp = cfg.journal.clone();
            if let Err(e) = recover_truncated_journal_tail_path(&jp) { return err(op, "JOURNAL_ERROR", &e) }
            if jp.exists() { if let Err(e) = read_journal_entries_path(&jp) { return err(op, "JOURNAL_ERROR", &e) } }
            let checks = match validate_move_plan(download_root.clone(), approved_root.clone(), vec![item.clone()]) {
                Ok(c) => c,
                Err(e) => return err(op, "VALIDATION_FAILED", &e),
            };
            if let Some(bad) = checks.iter().find(|x| !x.ok) {
                return err(op, "VALIDATION_FAILED", &bad.reason);
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
            // ---- N6 bridge-added: record moved-target snapshot for the Undo guard
            let size = std::fs::metadata(tp).map(|m| m.len()).ok();
            let modified = mtime_ms(tp);
            let _ = ledger_append(cfg, &json!({"kind": "moved_snapshot", "transaction_id": tx, "target": item.target,
                                                "size": size, "modified_ms": modified}));
            let _ = ledger_append(cfg, &json!({"kind": "approval_used", "approval_id": approval_id, "op": op, "transaction_id": tx}));
            ok(op, json!({"transaction_id": tx, "moved": 1, "failed": 0, "source": item.source, "target": item.target,
                          "target_size": size, "target_modified_ms": modified, "journal": jp,
                          "native_checks": ["validate_move_plan", "source_snapshot(size,modified_ms)", "revalidate_at_execution",
                                            "target_absent(no overwrite)", "approved_root_containment", "journal planned->moved"]}))
        }

        fn undo(v: &Value, cfg: &Config) -> Value {
            let op = "undo";
            let approval_id = match approval(v, op, cfg) { Ok(a) => a, Err(e) => return e };
            let tx = v.get("transaction_id").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            if tx.is_empty() { return err(op, "BAD_REQUEST", "transaction_id required") }
            let _guard = match lock(cfg) { Ok(g) => g, Err(e) => return err(op, "LOCKED", &e) };
            // ---- orchestration mirrored from FileOrbit undo_move_transaction (journal path = N6-A test journal)
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
            if !src.parent().map(|x| within(x, &cfg.source_roots)).unwrap_or(false) {
                return err(op, "ROOT_NOT_ALLOWED", "original location outside execution allowlist");
            }
            if !within(tgt, &cfg.approved_roots) { return err(op, "ROOT_NOT_ALLOWED", "moved target outside execution allowlist") }
            let undo_fail = |code: &str, msg: String| -> Value {
                let _ = append_journal_prevalidated_path(&p, &JournalEntry { status: "undo_failed".into(), timestamp: now_ms(), error: Some(msg.clone()), ..e.clone() });
                err(op, code, &msg)
            };
            // ---- N6 bridge-added guard: moved target must still match the snapshot recorded at Move
            let snap = ledger_lines(cfg).into_iter().rev()
                .find(|l| l["kind"] == "moved_snapshot" && l["transaction_id"] == tx.as_str() && l["target"] == e.target.as_str());
            let Some(snap) = snap else { return undo_fail("UNDO_TARGET_CHANGED", "no moved snapshot recorded - cannot verify current state".into()) };
            if !source_snapshot_matches(tgt, snap["size"].as_u64(), snap["modified_ms"].as_u64()) {
                return undo_fail("UNDO_TARGET_CHANGED", "Undo 중단: 이동된 파일이 Move 이후 변경됨 (size/modified_ms 불일치)".into());
            }
            if let Err(msg) = undo_precheck(src, tgt) { return undo_fail("UNDO_PRECHECK_FAILED", msg) }
            if let Some(parent) = src.parent() {
                if let Err(x) = std::fs::create_dir_all(parent) { return undo_fail("UNDO_FAILED", x.to_string()) }
            }
            if let Err(x) = move_with_audit(tgt, src,
                || append_journal_prevalidated_path(&p, &JournalEntry { status: "undone".into(), timestamp: now_ms(), error: None, ..e.clone() }), "Undo") {
                return undo_fail("UNDO_FAILED", x);
            }
            let _ = ledger_append(cfg, &json!({"kind": "approval_used", "approval_id": approval_id, "op": op, "transaction_id": tx}));
            ok(op, json!({"transaction_id": tx, "undone": 1, "restored": e.source, "from": e.target,
                          "restored_size": std::fs::metadata(src).map(|m| m.len()).ok(), "restored_modified_ms": mtime_ms(src)}))
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
    let journal = PathBuf::from(v.get("journal")?.as_str()?);
    Some(Config { source_roots: list("source_roots"), approved_roots: list("approved_roots"), journal })
}

fn main() {
    let mut req = String::new();
    let _ = std::io::stdin().read_to_string(&mut req);
    let out = match config() {
        Some(cfg) => exec::handle(&req, &cfg),
        None => serde_json::json!({"ok": false, "contract": exec::CONTRACT, "mode": "EXECUTION", "op": "",
                                   "error": "CONFIG_MISSING", "message": "fileorbit-execution-config.json required (fail closed)"}),
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
        let d = std::env::temp_dir().join(format!("fo-exec-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        for s in ["source", "destination", "journal"] {
            fs::create_dir_all(d.join(s)).unwrap();
        }
        let cfg = Config { source_roots: vec![d.join("source")], approved_roots: vec![d.join("destination")],
                           journal: d.join("journal").join("n6a-journal.jsonl") };
        (d, cfg)
    }

    fn snap(p: &Path) -> (u64, u64) {
        let m = fs::metadata(p).unwrap();
        (m.len(), m.modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64)
    }

    fn mv(d: &Path, name: &str, approval: &str, size: u64, mtime: u64) -> String {
        json!({"contract": "fileorbit-execution/1", "op": "safe_move", "approval_id": approval,
               "source_root": d.join("source"), "approved_root": d.join("destination"),
               "items": [{"source": d.join("source").join(name), "target": d.join("destination").join(name),
                          "expected_size": size, "expected_modified": mtime}]}).to_string()
    }

    fn undo(tx: &str, approval: &str) -> String {
        json!({"contract": "fileorbit-execution/1", "op": "undo", "approval_id": approval, "transaction_id": tx}).to_string()
    }

    #[test]
    fn exec_ops_allowlist_and_approval_required() {
        let (d, cfg) = env("ops");
        assert_eq!(handle(&json!({"contract": "fileorbit-execution/1", "op": "status"}).to_string(), &cfg)["result"]["ops"]
                   .as_array().unwrap().len(), 3);
        for op in ["scan", "move", "rename", "delete", "quarantine", "execute_move_plan", "export_journal", "hash"] {
            assert_eq!(handle(&json!({"contract": "fileorbit-execution/1", "op": op}).to_string(), &cfg)["error"], "OP_NOT_ALLOWED");
        }
        fs::write(d.join("source").join("a.txt"), b"a").unwrap();
        let (s, m) = snap(&d.join("source").join("a.txt"));
        assert_eq!(handle(&mv(&d, "a.txt", "", s, m), &cfg)["error"], "APPROVAL_REQUIRED");
        assert!(d.join("source").join("a.txt").exists() && !d.join("destination").join("a.txt").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn exec_move_undo_roundtrip_with_journal_and_replay_rejected() {
        let (d, cfg) = env("roundtrip");
        let src = d.join("source").join("n6.txt");
        fs::write(&src, b"n6a synthetic").unwrap();
        let (s, m) = snap(&src);
        let r = handle(&mv(&d, "n6.txt", "APR-1", s, m), &cfg);
        assert_eq!(r["ok"], true, "{r}");
        let tx = r["result"]["transaction_id"].as_str().unwrap().to_string();
        assert!(!src.exists() && fs::read(d.join("destination").join("n6.txt")).unwrap() == b"n6a synthetic");
        assert_eq!(handle(&mv(&d, "n6.txt", "APR-1", s, m), &cfg)["error"], "APPROVAL_ALREADY_USED");
        let u = handle(&undo(&tx, "APR-1"), &cfg);
        assert_eq!(u["ok"], true, "{u}");
        assert_eq!(fs::read(&src).unwrap(), b"n6a synthetic");
        assert!(!d.join("destination").join("n6.txt").exists());
        assert_eq!(handle(&undo(&tx, "APR-1"), &cfg)["error"], "APPROVAL_ALREADY_USED");
        let j = fs::read_to_string(&cfg.journal).unwrap();
        for st in ["\"planned\"", "\"moved\"", "\"undone\""] { assert!(j.contains(st), "{st}") }
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn exec_collision_and_source_change_blocked_without_mutation() {
        let (d, cfg) = env("collision");
        let src = d.join("source").join("c.txt");
        fs::write(&src, b"source").unwrap();
        fs::write(d.join("destination").join("c.txt"), b"existing").unwrap();
        let (s, m) = snap(&src);
        assert_eq!(handle(&mv(&d, "c.txt", "APR-C", s, m), &cfg)["error"], "VALIDATION_FAILED");
        assert_eq!(fs::read(d.join("destination").join("c.txt")).unwrap(), b"existing");
        fs::remove_file(d.join("destination").join("c.txt")).unwrap();
        assert_eq!(handle(&mv(&d, "c.txt", "APR-C2", s + 1, m), &cfg)["error"], "VALIDATION_FAILED"); // source snapshot changed
        assert!(src.exists() && !d.join("destination").join("c.txt").exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn exec_undo_target_changed_blocked() {
        let (d, cfg) = env("changed");
        let src = d.join("source").join("t.txt");
        fs::write(&src, b"original").unwrap();
        let (s, m) = snap(&src);
        let tx = handle(&mv(&d, "t.txt", "APR-T", s, m), &cfg)["result"]["transaction_id"].as_str().unwrap().to_string();
        fs::write(d.join("destination").join("t.txt"), b"modified after move").unwrap();
        assert_eq!(handle(&undo(&tx, "APR-T"), &cfg)["error"], "UNDO_TARGET_CHANGED");
        assert!(!src.exists());
        assert_eq!(fs::read(d.join("destination").join("t.txt")).unwrap(), b"modified after move");
        assert!(fs::read_to_string(&cfg.journal).unwrap().contains("\"undo_failed\""));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn exec_roots_outside_allowlist_blocked() {
        let (d, cfg) = env("roots");
        let other = std::env::temp_dir().join(format!("fo-exec-other-{}", std::process::id()));
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("x.txt"), b"x").unwrap();
        let (s, m) = snap(&other.join("x.txt"));
        let req = json!({"contract": "fileorbit-execution/1", "op": "safe_move", "approval_id": "APR-R",
                         "source_root": other, "approved_root": d.join("destination"),
                         "items": [{"source": other.join("x.txt"), "target": d.join("destination").join("x.txt"),
                                    "expected_size": s, "expected_modified": m}]}).to_string();
        assert_eq!(handle(&req, &cfg)["error"], "ROOT_NOT_ALLOWED");
        assert!(other.join("x.txt").exists());
        let _ = fs::remove_dir_all(other);
        let _ = fs::remove_dir_all(d);
    }
}
