use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tauri::Manager;
use walkdir::WalkDir;

const V1: &str = include_str!("../migrations/0001_index.sql");
const V2: &str = include_str!("../migrations/0002_scan_metadata.sql");

fn database_path(root: &Path) -> PathBuf { root.join("data").join("fileorbit.db") }

fn open_database(path: &Path) -> Result<Connection, String> {
    let parent = path.parent().ok_or("DB 상위 경로 없음")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("DB 경로 생성 실패: {e}"))?;
    let mut db = Connection::open(path).map_err(|e| format!("DB 열기 실패: {e}"))?;
    db.busy_timeout(std::time::Duration::from_millis(750)).map_err(|e| e.to_string())?;
    db.pragma_update(None, "foreign_keys", "ON").map_err(|e| e.to_string())?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(|e| format!("DB 버전 확인 실패: {e}"))?;
    if !(0..=2).contains(&version) { return Err(format!("지원하지 않는 DB schema version: {version}")); }
    let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(|e| format!("DB 무결성 확인 실패: {e}"))?;
    if check != "ok" { return Err(format!("DB 무결성 오류: {check}")); }
    for (number, sql) in [(1, V1), (2, V2)] {
        if version >= number { continue; }
        let tx = db.transaction().map_err(|e| format!("DB migration 시작 실패: {e}"))?;
        tx.execute_batch(sql).map_err(|e| format!("DB migration {number} 실패: {e}"))?;
        tx.execute("INSERT INTO schema_migrations(version) VALUES (?1)", [number]).map_err(|e| format!("DB migration 기록 실패: {e}"))?;
        tx.pragma_update(None, "user_version", number).map_err(|e| format!("DB migration 버전 기록 실패: {e}"))?;
        tx.commit().map_err(|e| format!("DB migration commit 실패: {e}"))?;
    }
    Ok(db)
}

pub fn initialize_app(app: &tauri::AppHandle) -> Result<(), String> {
    // Until the separately gated C:\FileOrbit migration, retain the app's existing data root.
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    open_database(&database_path(&root)).map(|_| ())
}

fn test_database(test_root: &str) -> Result<Connection, String> {
    let paths = crate::paths::bootstrap_test_root(test_root, false)?;
    open_database(Path::new(&paths.database))
}

fn checked_scan_root(test_root: &str, scan_root: &str) -> Result<PathBuf, String> {
    let test = crate::paths::bootstrap_test_root(test_root, true)?;
    let test = Path::new(&test.root).canonicalize().map_err(|e| format!("Test Root 확인 실패: {e}"))?;
    let fixtures = test.join("testdata").canonicalize().map_err(|e| format!("testdata 확인 실패: {e}"))?;
    let scan = Path::new(scan_root).canonicalize().map_err(|e| format!("스캔 경로 확인 실패: {e}"))?;
    if !scan.is_dir() || !scan.starts_with(&fixtures) { return Err("스캔은 Test Root/testdata 안의 폴더만 허용합니다".into()); }
    Ok(scan)
}

fn ns(time: std::io::Result<SystemTime>) -> i64 {
    time.ok().and_then(|v| v.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos().min(i64::MAX as u128) as i64).unwrap_or(0)
}
fn stamp() -> String { format!("{}", ns(Ok(SystemTime::now()))) }

fn safe_entry(entry: &walkdir::DirEntry) -> bool {
    if entry.file_type().is_symlink() { return false; }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(entry.path()) {
            if meta.file_attributes() & 0x400 != 0 { return false; }
        }
    }
    true
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus { pub schema_version: i64, pub file_count: i64, pub last_scan: Option<String> }

fn status(db: &Connection) -> Result<IndexStatus, String> {
    Ok(IndexStatus {
        schema_version: db.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(|e| e.to_string())?,
        file_count: db.query_row("SELECT count(*) FROM files WHERE state='present'", [], |r| r.get(0)).map_err(|e| e.to_string())?,
        last_scan: db.query_row("SELECT completed_at FROM scan_runs WHERE status='completed' ORDER BY completed_at DESC LIMIT 1", [], |r| r.get(0)).optional().map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub fn index_status(test_root: String) -> Result<IndexStatus, String> { status(&test_database(&test_root)?) }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedFile { pub path: String, pub name: String, pub extension: Option<String>, pub size_bytes: i64, pub state: String }

#[tauri::command]
pub fn indexed_files(test_root: String, name: Option<String>, path_prefix: Option<String>, extension: Option<String>, limit: Option<u32>) -> Result<Vec<IndexedFile>, String> {
    let db = test_database(&test_root)?;
    let mut stmt = db.prepare("SELECT path,name,extension,size_bytes,state FROM files WHERE (?1 IS NULL OR instr(lower(name),lower(?1))>0) AND (?2 IS NULL OR substr(path,1,length(?2))=?2) AND (?3 IS NULL OR extension=?3) ORDER BY path LIMIT ?4").map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![name,path_prefix,extension,limit.unwrap_or(100).clamp(1,1000)], |r| Ok(IndexedFile {path:r.get(0)?,name:r.get(1)?,extension:r.get(2)?,size_bytes:r.get(3)?,state:r.get(4)?})).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_indexed_test_root(test_root: String, scan_root: String) -> Result<IndexStatus, String> {
    let root = checked_scan_root(&test_root, &scan_root)?;
    let mut entries = Vec::new();
    for item in WalkDir::new(&root).follow_links(false).into_iter().filter_entry(safe_entry) {
        let item = item.map_err(|e| format!("스캔 읽기 실패: {e}"))?;
        let path = item.path().to_path_buf();
        let meta = item.metadata().map_err(|e| format!("스캔 metadata 실패: {e}"))?;
        entries.push((path, item.file_type().is_dir(), meta.len() as i64, ns(meta.modified()), ns(meta.created())));
    }
    let mut db = test_database(&test_root)?;
    let tx = db.transaction().map_err(|e| format!("스캔 transaction 실패: {e}"))?;
    let root_path = root.to_string_lossy().into_owned();
    let run_id = format!("scan-{}", stamp());
    tx.execute("INSERT OR IGNORE INTO scan_roots(id,path) VALUES(?1,?1)", [&root_path]).map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO scan_runs(id,root_id,started_at,status) VALUES(?1,?2,?3,'running')", params![run_id,root_path,stamp()]).map_err(|e| e.to_string())?;
    let mut folders = 0;
    let mut files = 0;
    for (path, is_dir, size, modified, created) in entries {
        let path_str = path.to_string_lossy().into_owned();
        let relative = path.strip_prefix(&root).map_err(|e| e.to_string())?.to_string_lossy().into_owned();
        let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if is_dir {
            let parent = if path == root { None } else { path.parent().map(|p| p.to_string_lossy().into_owned()) };
            tx.execute("INSERT INTO folders(id,root_id,path,parent_id,name,state,last_seen_run_id,relative_path) VALUES(?1,?2,?1,?3,?4,'present',?5,?6) ON CONFLICT(path) DO UPDATE SET state='present',last_seen_run_id=excluded.last_seen_run_id,name=excluded.name",params![path_str,root_path,parent,name,run_id,relative]).map_err(|e| format!("폴더 저장 실패: {e}"))?;
            folders += 1;
        } else {
            let parent = path.parent().ok_or("파일 상위 폴더 없음")?.to_string_lossy().into_owned();
            let ext = path.extension().map(|v| v.to_string_lossy().to_ascii_lowercase());
            tx.execute("INSERT INTO files(id,root_id,folder_id,path,name,extension,size_bytes,modified_ns,state,last_seen_run_id,relative_path,created_ns) VALUES(?1,?2,?3,?1,?4,?5,?6,?7,'present',?8,?9,?10) ON CONFLICT(path) DO UPDATE SET name=excluded.name,extension=excluded.extension,size_bytes=excluded.size_bytes,modified_ns=excluded.modified_ns,created_ns=excluded.created_ns,state='present',last_seen_run_id=excluded.last_seen_run_id,changed_at=CURRENT_TIMESTAMP",params![path_str,root_path,parent,name,ext,size,modified,run_id,relative,created]).map_err(|e| format!("파일 저장 실패: {e}"))?;
            files += 1;
        }
    }
    tx.execute("UPDATE files SET state='missing' WHERE root_id=?1 AND last_seen_run_id<>?2 AND state='present'",params![root_path,run_id]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE folders SET state='missing' WHERE root_id=?1 AND last_seen_run_id<>?2 AND state='present'",params![root_path,run_id]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE scan_runs SET completed_at=?2,status='completed',files_seen=?3,folders_seen=?4 WHERE id=?1",params![run_id,stamp(),files,folders]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE scan_roots SET last_completed_run_id=?2 WHERE id=?1",params![root_path,run_id]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| format!("스캔 commit 실패: {e}"))?;
    status(&db)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_root_scan_persists_and_rescans() {
        let root = std::env::temp_dir().join(format!("fileorbit-index-{}-{}",std::process::id(),stamp()));
        let root_str = root.to_string_lossy().into_owned();
        crate::paths::bootstrap_test_root(&root_str,false).unwrap();
        let fixtures = root.join("testdata/fixtures");
        std::fs::create_dir_all(fixtures.join("D-Project/회의록")).unwrap();
        let file = fixtures.join("D-Project/회의록/회의 (1).TXT");
        std::fs::write(&file,b"synthetic").unwrap();
        let scan = fixtures.to_string_lossy().into_owned();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,1);
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,1);
        assert_eq!(indexed_files(root_str.clone(),Some("회의".into()),None,Some("txt".into()),None).unwrap().len(),1);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan).unwrap().file_count,0);
        assert_eq!(indexed_files(root_str.clone(),None,None,None,None).unwrap()[0].state,"missing");
        assert_eq!(test_database(&root_str).unwrap().query_row("SELECT count(*) FROM files",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
