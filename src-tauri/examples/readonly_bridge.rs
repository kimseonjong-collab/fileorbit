//! KSJ Nexus read-only bridge for FileOrbit V0.5 (contract `fileorbit-readonly/1`).
//!
//! - Reuses FileOrbit's own `scan_folder` / `scan_downloads` by including the UNMODIFIED `src/lib.rs`
//!   (lib.rs stays byte-identical to RC2). No Tauri runtime is started, no AppHandle exists.
//! - Exactly four operations are routed: status, scan, folder_index, metadata. Everything else is
//!   `OP_NOT_ALLOWED`; `hash` is `NOT_PROVIDED_BY_FILEORBIT`. No route reaches Move/Undo/Journal code.
//! - Roots must be inside an allowlisted test root (`fileorbit-bridge-allowlist.json` next to the exe);
//!   a missing/empty allowlist rejects every path (`ROOT_NOT_ALLOWED`).
//! - One JSON request on stdin, one JSON response on stdout.
#![allow(dead_code, unused_imports)]

mod fileorbit {
    include!("../src/lib.rs");

    pub mod bridge {
        use super::*;
        use serde_json::{json, Value};
        use std::path::PathBuf;

        pub const CONTRACT: &str = "fileorbit-readonly/1";
        pub const OPS: [&str; 4] = ["status", "scan", "folder_index", "metadata"];

        fn canon(p: &Path) -> Option<PathBuf> {
            std::fs::canonicalize(p).ok()
        }

        fn allowed(p: &Path, roots: &[PathBuf]) -> bool {
            let Some(c) = canon(p) else { return false };
            roots.iter().filter_map(|r| canon(r)).any(|r| c.starts_with(&r))
        }

        fn err(op: &str, code: &str, msg: &str) -> Value {
            json!({"ok": false, "contract": CONTRACT, "mode": "READ_ONLY", "op": op, "error": code, "message": msg})
        }

        fn ok(op: &str, result: Value) -> Value {
            json!({"ok": true, "contract": CONTRACT, "mode": "READ_ONLY",
                   "fileorbit_version": env!("CARGO_PKG_VERSION"), "op": op, "result": result})
        }

        pub fn handle(req: &str, roots: &[PathBuf]) -> Value {
            let v: Value = match serde_json::from_str(req) {
                Ok(v) => v,
                Err(e) => return err("", "BAD_REQUEST", &e.to_string()),
            };
            if v.get("contract").and_then(|c| c.as_str()) != Some(CONTRACT) {
                return err("", "BAD_REQUEST", "contract mismatch");
            }
            let op = v.get("op").and_then(|o| o.as_str()).unwrap_or("");
            match op {
                "status" => ok(op, json!({
                    "ops": OPS, "hash": "NOT_PROVIDED_BY_FILEORBIT", "allowed_root_count": roots.len(),
                    "reused": ["scan_folder", "scan_downloads"],
                    "runtime": "no Tauri runtime; FileOrbit V0.5 lib.rs included unmodified"})),
                "hash" => err(op, "NOT_PROVIDED_BY_FILEORBIT", "FileOrbit V0.5 has no hash function"),
                "scan" | "folder_index" => {
                    let Some(root) = v.get("root").and_then(|r| r.as_str()) else {
                        return err(op, "BAD_REQUEST", "root required");
                    };
                    if !allowed(Path::new(root), roots) {
                        return err(op, "ROOT_NOT_ALLOWED", root);
                    }
                    match scan_folder(root.to_string()) {
                        Ok(s) if op == "scan" => ok(op, serde_json::to_value(&s).unwrap_or(Value::Null)),
                        Ok(s) => ok(op, Value::Array(s.folders.iter().map(|f| json!({
                            "path": f.path, "name": f.name, "fileCount": f.file_count,
                            "extensions": f.extensions})).collect())),
                        Err(e) => err(op, "SCAN_FAILED", &e),
                    }
                }
                "metadata" => {
                    let Some(path) = v.get("path").and_then(|r| r.as_str()) else {
                        return err(op, "BAD_REQUEST", "path required");
                    };
                    let p = Path::new(path);
                    if !allowed(p, roots) || !p.is_file() {
                        return err(op, "ROOT_NOT_ALLOWED", path);
                    }
                    let (Some(parent), Some(target)) = (p.parent(), canon(p)) else {
                        return err(op, "BAD_REQUEST", "invalid path");
                    };
                    match scan_downloads(parent.to_string_lossy().to_string()) {
                        Ok(files) => match files.into_iter().find(|f| canon(Path::new(&f.path)).as_ref() == Some(&target)) {
                            Some(f) => ok(op, serde_json::to_value(&f).unwrap_or(Value::Null)),
                            None => err(op, "NOT_FOUND", path),
                        },
                        Err(e) => err(op, "SCAN_FAILED", &e),
                    }
                }
                _ => err(op, "OP_NOT_ALLOWED", op),
            }
        }
    }
}

use fileorbit::bridge;
use std::io::Read;
use std::path::PathBuf;

fn allowlist() -> Vec<PathBuf> {
    let file = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("fileorbit-bridge-allowlist.json")));
    let Some(text) = file.and_then(|f| std::fs::read_to_string(f).ok()) else { return vec![] };
    serde_json::from_str::<serde_json::Value>(&text).ok()
        .and_then(|v| v.get("allowed_roots").cloned())
        .and_then(|a| a.as_array().cloned())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(PathBuf::from)).collect())
        .unwrap_or_default()
}

fn main() {
    let mut req = String::new();
    let _ = std::io::stdin().read_to_string(&mut req);
    let out = bridge::handle(&req, &allowlist());
    println!("{}", out);
    std::process::exit(if out.get("ok").and_then(|o| o.as_bool()) == Some(true) { 0 } else { 2 });
}

#[cfg(test)]
mod tests {
    use super::fileorbit::bridge::{handle, OPS};
    use std::fs;
    use std::path::PathBuf;

    fn req(op: &str, key: &str, val: &str) -> String {
        serde_json::json!({"contract": "fileorbit-readonly/1", "op": op, key: val}).to_string()
    }

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("fo-bridge-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn bridge_only_four_ops_and_hash_not_provided() {
        let s = handle(&req("status", "x", ""), &[]);
        assert_eq!(s["ok"], true);
        assert_eq!(s["result"]["ops"].as_array().unwrap().len(), 4);
        assert_eq!(OPS, ["status", "scan", "folder_index", "metadata"]);
        for op in ["move", "rename", "delete", "quarantine", "undo", "validate_move_plan", "execute_move_plan",
                   "undo_move_transaction", "export_journal", ""] {
            assert_eq!(handle(&req(op, "root", "."), &[])["error"], "OP_NOT_ALLOWED", "{op}");
        }
        assert_eq!(handle(&req("hash", "path", "."), &[])["error"], "NOT_PROVIDED_BY_FILEORBIT");
        assert_eq!(handle("{\"op\":\"status\"}", &[])["error"], "BAD_REQUEST");
    }

    #[test]
    fn bridge_rejects_roots_outside_allowlist() {
        let allowed = tmp("allowed");
        let other = tmp("other");
        fs::write(other.join("secret.txt"), b"x").unwrap();
        let roots = vec![allowed.clone()];
        let escape = allowed.join("..").join(other.file_name().unwrap());
        for r in [other.to_string_lossy().to_string(), escape.to_string_lossy().to_string()] {
            assert_eq!(handle(&req("scan", "root", &r), &roots)["error"], "ROOT_NOT_ALLOWED");
            assert_eq!(handle(&req("folder_index", "root", &r), &roots)["error"], "ROOT_NOT_ALLOWED");
        }
        let f = other.join("secret.txt").to_string_lossy().to_string();
        assert_eq!(handle(&req("metadata", "path", &f), &roots)["error"], "ROOT_NOT_ALLOWED");
        assert_eq!(handle(&req("scan", "root", &allowed.to_string_lossy()), &[])["error"], "ROOT_NOT_ALLOWED");
        let _ = fs::remove_dir_all(allowed);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn bridge_scan_index_metadata_are_read_only() {
        let root = tmp("ro");
        let dl = root.join("Downloads");
        fs::create_dir_all(&dl).unwrap();
        let file = dl.join("TEMPO note.txt");
        fs::write(&file, b"unchanged").unwrap();
        let before = fs::read(&file).unwrap();
        let roots = vec![root.clone()];
        let s = handle(&req("scan", "root", &root.to_string_lossy()), &roots);
        assert_eq!(s["ok"], true);
        assert_eq!(s["result"]["fileCount"], 1);
        let i = handle(&req("folder_index", "root", &root.to_string_lossy()), &roots);
        assert_eq!(i["result"].as_array().unwrap().len(), 1);
        let m = handle(&req("metadata", "path", &file.to_string_lossy()), &roots);
        assert_eq!(m["result"]["size"], 9);
        assert_eq!(fs::read(&file).unwrap(), before);
        assert_eq!(fs::read_dir(&dl).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
