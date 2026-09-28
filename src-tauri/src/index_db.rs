use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tauri::Manager;
use walkdir::WalkDir;

const V1: &str = include_str!("../migrations/0001_index.sql");
const V2: &str = include_str!("../migrations/0002_scan_metadata.sql");
const V3: &str = include_str!("../migrations/0003_inbox.sql");
const V4: &str = include_str!("../migrations/0004_workspace_corrections.sql");

fn database_path(root: &Path) -> PathBuf { root.join("data").join("fileorbit.db") }

fn open_database(path: &Path) -> Result<Connection, String> {
    let parent = path.parent().ok_or("DB 상위 경로 없음")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("DB 경로 생성 실패: {e}"))?;
    let mut db = Connection::open(path).map_err(|e| format!("DB 열기 실패: {e}"))?;
    db.busy_timeout(std::time::Duration::from_millis(750)).map_err(|e| e.to_string())?;
    db.pragma_update(None, "foreign_keys", "ON").map_err(|e| e.to_string())?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0)).map_err(|e| format!("DB 버전 확인 실패: {e}"))?;
    if !(0..=4).contains(&version) { return Err(format!("지원하지 않는 DB schema version: {version}")); }
    let check: String = db.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(|e| format!("DB 무결성 확인 실패: {e}"))?;
    if check != "ok" { return Err(format!("DB 무결성 오류: {check}")); }
    for (number, sql) in [(1, V1), (2, V2), (3, V3), (4, V4)] {
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
    checked_test_root(test_root)?;
    let paths = crate::paths::bootstrap_test_root(test_root, false)?;
    open_database(Path::new(&paths.database))
}

fn checked_test_root(test_root: &str) -> Result<(), String> {
    let candidate = crate::paths::bootstrap_test_root(test_root, true)?;
    let root = Path::new(&candidate.root);
    let temp = std::env::temp_dir().canonicalize().map_err(|e| e.to_string())?;
    if root.starts_with(&temp) { return Ok(()); }
    #[cfg(windows)]
    {
        let normalized = root.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
        if normalized.starts_with(r"\\?\c:\fileorbit-test\") || normalized.starts_with(r"c:\fileorbit-test\") {
            return Ok(());
        }
    }
    Err("Index Test Root는 OS 임시 폴더 또는 C:\\FileOrbit-Test 아래에만 허용합니다".into())
}

fn checked_scan_root(test_root: &str, scan_root: &str) -> Result<PathBuf, String> {
    checked_test_root(test_root)?;
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
    if entry.depth() > 0 && entry.file_type().is_dir() && crate::excluded(&entry.file_name().to_string_lossy()) { return false; }
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

#[tauri::command]
pub fn app_index_status(app: tauri::AppHandle) -> Result<IndexStatus, String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    status(&open_database(&database_path(&root))?)
}

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
    tx.execute_batch("CREATE TEMP TABLE seen_files(path TEXT PRIMARY KEY); CREATE TEMP TABLE seen_folders(path TEXT PRIMARY KEY);").map_err(|e| format!("스캔 상태 준비 실패: {e}"))?;
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
            tx.execute("INSERT INTO temp.seen_folders(path) VALUES(?1)",[&path_str]).map_err(|e| e.to_string())?;
            let parent = if path == root { None } else { path.parent().map(|p| p.to_string_lossy().into_owned()) };
            tx.execute("INSERT INTO folders(id,root_id,path,parent_id,name,state,last_seen_run_id,relative_path) VALUES(?1,?2,?1,?3,?4,'present',?5,?6) ON CONFLICT(path) DO UPDATE SET state='present',last_seen_run_id=excluded.last_seen_run_id,name=excluded.name,parent_id=excluded.parent_id WHERE folders.state!='present' OR folders.name!=excluded.name OR folders.parent_id IS NOT excluded.parent_id",params![path_str,root_path,parent,name,run_id,relative]).map_err(|e| format!("폴더 저장 실패: {e}"))?;
            folders += 1;
        } else {
            tx.execute("INSERT INTO temp.seen_files(path) VALUES(?1)",[&path_str]).map_err(|e| e.to_string())?;
            let parent = path.parent().ok_or("파일 상위 폴더 없음")?.to_string_lossy().into_owned();
            let ext = path.extension().map(|v| v.to_string_lossy().to_ascii_lowercase());
            tx.execute("INSERT INTO files(id,root_id,folder_id,path,name,extension,size_bytes,modified_ns,state,last_seen_run_id,relative_path,created_ns) VALUES(?1,?2,?3,?1,?4,?5,?6,?7,'present',?8,?9,?10) ON CONFLICT(path) DO UPDATE SET name=excluded.name,extension=excluded.extension,size_bytes=excluded.size_bytes,modified_ns=excluded.modified_ns,created_ns=excluded.created_ns,state='present',last_seen_run_id=excluded.last_seen_run_id,changed_at=CURRENT_TIMESTAMP WHERE files.state!='present' OR files.name!=excluded.name OR files.extension IS NOT excluded.extension OR files.size_bytes!=excluded.size_bytes OR files.modified_ns!=excluded.modified_ns OR files.created_ns IS NOT excluded.created_ns OR files.folder_id!=excluded.folder_id",params![path_str,root_path,parent,name,ext,size,modified,run_id,relative,created]).map_err(|e| format!("파일 저장 실패: {e}"))?;
            files += 1;
        }
    }
    tx.execute("UPDATE files SET state='missing' WHERE root_id=?1 AND state='present' AND NOT EXISTS(SELECT 1 FROM temp.seen_files WHERE path=files.path)",[&root_path]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE folders SET state='missing' WHERE root_id=?1 AND state='present' AND NOT EXISTS(SELECT 1 FROM temp.seen_folders WHERE path=folders.path)",[&root_path]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE scan_runs SET completed_at=?2,status='completed',files_seen=?3,folders_seen=?4 WHERE id=?1",params![run_id,stamp(),files,folders]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE scan_roots SET last_completed_run_id=?2 WHERE id=?1",params![root_path,run_id]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| format!("스캔 commit 실패: {e}"))?;
    status(&db)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItem {
    pub file_id: String, pub filename: String, pub current_path: String,
    pub extension: Option<String>, pub size_bytes: i64, pub modified_ns: i64,
    pub modified_ns_text: String,
    pub index_state: String, pub review_state: String, pub related_state: String,
    pub proposed_destination: Option<String>, pub reason: Option<String>,
    pub confidence: Option<f64>, pub action_status: String,
}

// The caller supplies only a disposable Test Root folder. The existing Downloads UI remains intact.
#[tauri::command]
pub fn discover_test_inbox(test_root: String, inbox_path: String) -> Result<Vec<InboxItem>, String> {
    let inbox = checked_scan_root(&test_root, &inbox_path)?;
    scan_indexed_test_root(test_root.clone(), inbox.to_string_lossy().into_owned())?;
    let db = test_database(&test_root)?;
    db.execute("INSERT OR IGNORE INTO inbox_items(file_id) SELECT id FROM files WHERE folder_id=?1 AND state='present'", [inbox.to_string_lossy().as_ref()]).map_err(|e| format!("Inbox 저장 실패: {e}"))?;
    list_test_inbox(test_root)
}

#[tauri::command]
pub fn list_test_inbox(test_root: String) -> Result<Vec<InboxItem>, String> {
    let db = test_database(&test_root)?;
    let mut stmt = db.prepare("SELECT f.id,f.name,f.path,f.extension,f.size_bytes,f.modified_ns,f.state,i.review_state,i.related_state,i.proposed_destination,i.reason,i.confidence,i.action_status FROM inbox_items i JOIN files f ON f.id=i.file_id ORDER BY f.path").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(InboxItem { file_id:r.get(0)?, filename:r.get(1)?, current_path:r.get(2)?, extension:r.get(3)?, size_bytes:r.get(4)?, modified_ns:r.get(5)?, modified_ns_text:r.get::<_,i64>(5)?.to_string(), index_state:r.get(6)?, review_state:r.get(7)?, related_state:r.get(8)?, proposed_destination:r.get(9)?, reason:r.get(10)?, confidence:r.get(11)?, action_status:r.get(12)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedCandidate { pub candidate: String, pub candidate_type: String, pub score: f64, pub evidence: String }

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|s| s.chars().count() >= 2).map(str::to_owned).collect()
}

#[tauri::command]
pub fn indexed_candidates(test_root: String, file_id: String, limit: Option<u32>) -> Result<Vec<RelatedCandidate>, String> {
    let db = test_database(&test_root)?;
    let (name, ext, modified): (String,Option<String>,i64) = db.query_row("SELECT name,extension,modified_ns FROM files WHERE id=?1 AND state='present'", [&file_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e| format!("Index file ID 조회 실패: {e}"))?;
    let words = tokens(&name);
    let mut candidates = Vec::new();
    let mut stmt = db.prepare("SELECT path,name,'folder',NULL,0 FROM folders WHERE state='present' UNION ALL SELECT path,name,'file',extension,modified_ns FROM files WHERE state='present' AND id!=?1").map_err(|e|e.to_string())?;
    let rows = stmt.query_map([&file_id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,i64>(4)?))).map_err(|e|e.to_string())?;
    for row in rows {
        let (path, candidate_name, kind, candidate_ext, candidate_modified) = row.map_err(|e|e.to_string())?;
        let hay = tokens(&format!("{path} {candidate_name}"));
        let common = words.iter().filter(|w| hay.contains(w)).count();
        let same_ext = kind == "file" && ext.is_some() && ext == candidate_ext;
        let near_date = kind == "file" && modified.abs_diff(candidate_modified) < 7 * 24 * 3600 * 1_000_000_000;
        let score = (common as f64 * 0.25 + if same_ext {0.10} else {0.0} + if near_date {0.05} else {0.0}).min(1.0);
        if score > 0.0 { candidates.push(RelatedCandidate { candidate:path, candidate_type:kind, score, evidence:format!("공통 단어 {common}개; 확장자 일치: {same_ext}; 날짜 근접: {near_date}") }); }
    }
    candidates.sort_by(|a,b| b.score.total_cmp(&a.score).then_with(||a.candidate.cmp(&b.candidate)));
    candidates.truncate(limit.unwrap_or(10).clamp(1,100) as usize);
    Ok(candidates)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRow {
    pub stable_item_id: String, pub file_id: String, pub current_path: String,
    pub filename: String, pub extension: Option<String>, pub size_bytes: i64,
    pub modified_ns: i64, pub index_state: String, pub review_state: String,
    pub related_candidate_status: String, pub candidate_folder: Option<String>,
    pub related_files: Option<String>, pub recommendation: Option<String>,
    pub reason: Option<String>, pub confidence: Option<f64>,
    pub user_correction: Option<String>, pub final_destination: Option<String>,
    pub action: Option<String>, pub action_status: String, pub batch_id: Option<String>,
    pub sync_status: String, pub updated_at: String,
}

// Export-only adapter: no Google authentication, filesystem command, or Sheet value is trusted here.
#[tauri::command]
pub fn test_workspace_rows(test_root: String) -> Result<Vec<WorkspaceRow>, String> {
    let db = test_database(&test_root)?;
    let mut stmt = db.prepare("SELECT f.id,f.path,f.name,f.extension,f.size_bytes,f.modified_ns,f.state,i.review_state,i.related_state,i.proposed_destination,i.reason,i.confidence,i.action_status,i.updated_at FROM inbox_items i JOIN files f ON f.id=i.file_id ORDER BY f.id").map_err(|e|e.to_string())?;
    let rows = stmt.query_map([], |r| {
        let id: String = r.get(0)?;
        Ok(WorkspaceRow { stable_item_id:format!("inbox:{id}"), file_id:id, current_path:r.get(1)?, filename:r.get(2)?, extension:r.get(3)?, size_bytes:r.get(4)?, modified_ns:r.get(5)?, index_state:r.get(6)?, review_state:r.get(7)?, related_candidate_status:r.get(8)?, candidate_folder:None, related_files:None, recommendation:None, reason:r.get(10)?, confidence:r.get(11)?, user_correction:None, final_destination:r.get(9)?, action:None, action_status:r.get(12)?, batch_id:None, sync_status:"not_synced".into(), updated_at:r.get(13)? })
    }).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSyncPlan {
    pub report: crate::workspace_sync::SyncReport,
    pub rows: Vec<crate::workspace_sync::ReviewRow>,
}

// The caller may supply a bounded Sheet snapshot. This command returns an idempotent plan only.
#[tauri::command]
pub fn plan_test_workspace_sync(test_root: String, remote_rows: Vec<crate::workspace_sync::ReviewRow>) -> Result<WorkspaceSyncPlan, String> {
    if remote_rows.len() > 10_000 { return Err("Sheet 행 제한 초과".into()); }
    let local: Vec<_> = test_workspace_rows(test_root)?.into_iter().map(|r| crate::workspace_sync::ReviewRow {
        stable_item_id:r.stable_item_id, file_id:r.file_id, current_path:r.current_path,
        filename:r.filename, size_bytes:r.size_bytes, modified_ns:r.modified_ns,
        index_state:r.index_state, review_state:r.review_state,
        user_correction:String::new(), action_status:r.action_status,
    }).collect();
    let mut provider = crate::workspace_sync::MemorySheet { rows:remote_rows };
    let report = crate::workspace_sync::sync_inbox(&mut provider,&local)?;
    Ok(WorkspaceSyncPlan { report, rows:provider.rows })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSyncPlan {
    pub report: crate::workspace_sync::SyncReport,
    pub rows: Vec<crate::workspace_sync::CandidateRow>,
}

#[tauri::command]
pub fn plan_test_candidate_sync(test_root: String, file_id: String, remote_rows: Vec<crate::workspace_sync::CandidateRow>) -> Result<CandidateSyncPlan, String> {
    if remote_rows.len() > 10_000 { return Err("Sheet 후보 행 제한 초과".into()); }
    let local = indexed_candidates(test_root, file_id.clone(),Some(100))?.into_iter().map(|c| crate::workspace_sync::CandidateRow {
        stable_item_id:format!("inbox:{file_id}"),file_id:file_id.clone(),
        candidate_id:format!("candidate:{}:{}:{}",file_id,c.candidate_type,c.candidate),
        candidate_type:c.candidate_type,candidate_path:c.candidate,
        score_basis_points:(c.score * 10_000.0).round() as u16,evidence:c.evidence,
    }).collect::<Vec<_>>();
    let (report,rows) = crate::workspace_sync::plan_candidates(&local,&remote_rows)?;
    Ok(CandidateSyncPlan { report, rows })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionPreview {
    pub file_id: String, pub action: String, pub source_path: String,
    pub destination_path: String, pub user_correction: String,
    pub validation_status: String, pub dry_run_status: String,
}

// Natural language is context only. An explicit structured destination is required and no move is called.
#[tauri::command]
pub fn preview_test_correction(test_root: String, file_id: String, user_correction: String, destination_path: String) -> Result<CorrectionPreview, String> {
    let db = test_database(&test_root)?;
    let source: String = db.query_row("SELECT path FROM files WHERE id=?1 AND state='present'", [&file_id], |r|r.get(0)).map_err(|e|format!("Index file ID 조회 실패: {e}"))?;
    let src = Path::new(&source);
    if !src.is_file() || src.symlink_metadata().map_err(|e|e.to_string())?.file_type().is_symlink() { return Err("원본 파일 상태가 Index와 다릅니다. 재스캔이 필요합니다".into()); }
    let target = Path::new(&destination_path);
    if !target.is_absolute() || crate::has_parent_dir(target) || crate::windows_reserved_target_name(target) || target.symlink_metadata().is_ok() { return Err("목적지는 충돌 없는 절대 경로여야 합니다".into()); }
    let parent = target.parent().ok_or("목적지 상위 폴더 없음")?;
    let parent = parent.canonicalize().map_err(|e|format!("목적지 상위 폴더 확인 실패: {e}"))?;
    let root = crate::paths::bootstrap_test_root(&test_root,true)?;
    let fixture = Path::new(&root.root).join("testdata").canonicalize().map_err(|e|e.to_string())?;
    if !src.canonicalize().map_err(|e|e.to_string())?.starts_with(&fixture) { return Err("원본이 Test Root 밖을 가리킵니다".into()); }
    if !parent.starts_with(&fixture) || parent == fixture { return Err("목적지는 Test Root/testdata의 기존 하위 폴더 안에 있어야 합니다".into()); }
    let filename = target.file_name().ok_or("목적지 파일명 없음")?;
    let normalized_target = parent.join(filename);
    if normalized_target.symlink_metadata().is_ok() { return Err("목적지 충돌".into()); }
    Ok(CorrectionPreview { file_id, action:"MOVE".into(), source_path:source, destination_path:normalized_target.to_string_lossy().into_owned(), user_correction, validation_status:"validated_test_root".into(), dry_run_status:"preview_only".into() })
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetCorrection {
    pub stable_item_id: String, pub file_id: String, pub correction_revision: String,
    pub user_correction: String, pub normalized_action: String, pub source_path: String,
    pub destination_path: Option<String>, pub snapshot_size_bytes: String,
    pub snapshot_modified_ns: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedCorrection {
    pub stable_item_id: String, pub file_id: String, pub normalized_action: String,
    pub source_path: String, pub destination_path: Option<String>, pub status: String,
}

// Shared, read-only safety gate for import, single dry-run and batch dry-run.
// It is deliberately scoped to disposable Test Root data and returns no executable capability.
fn validate_correction_rows(db: &Connection, test_root: &str, corrections: &[SheetCorrection]) -> Result<Vec<Option<String>>, String> {
    let mut ids = std::collections::HashSet::new();
    let mut destinations = Vec::with_capacity(corrections.len());
    for row in corrections {
        if !ids.insert(&row.stable_item_id) { return Err("중복 수정 item ID".into()); }
        if row.stable_item_id != format!("inbox:{}",row.file_id) || row.correction_revision.trim().is_empty() || row.user_correction.trim().is_empty() {
            return Err("수정 ID, revision 또는 내용이 비어 있거나 일치하지 않습니다".into());
        }
        if !matches!(row.normalized_action.as_str(),"MOVE"|"HOLD") { return Err("지원하지 않는 구조화 action".into()); }
        let (path,size,modified,state): (String,i64,i64,String) = db.query_row("SELECT path,size_bytes,modified_ns,state FROM files WHERE id=?1",[&row.file_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|_|"알 수 없는 file ID".to_string())?;
        let expected_size = row.snapshot_size_bytes.parse::<i64>().map_err(|_|"잘못된 파일 크기 snapshot".to_string())?;
        let expected_modified = row.snapshot_modified_ns.parse::<i64>().map_err(|_|"잘못된 수정시각 snapshot".to_string())?;
        if state != "present" || path != row.source_path || size != expected_size || modified != expected_modified { return Err("Sheet 수정이 현재 SQLite Index와 충돌합니다. 재검토가 필요합니다".into()); }
        let source = Path::new(&path);
        let live = source.symlink_metadata().map_err(|_|"원본 파일이 없습니다".to_string())?;
        let fixture = Path::new(&crate::paths::bootstrap_test_root(test_root,true)?.root).join("testdata").canonicalize().map_err(|e|e.to_string())?;
        if !live.file_type().is_file() || !source.canonicalize().map_err(|e|e.to_string())?.starts_with(&fixture)
            || live.len() as i64 != size || ns(live.modified()) != modified { return Err("원본 파일이 스캔 이후 변경되었거나 Test Root를 벗어났습니다".into()); }
        let destination = if row.normalized_action == "MOVE" {
            let target = row.destination_path.as_deref().filter(|s|!s.trim().is_empty()).ok_or("MOVE 목적지 없음")?;
            let preview = preview_test_correction(test_root.into(),row.file_id.clone(),row.user_correction.clone(),target.into())?;
            if preview.destination_path == path { return Err("원본과 목적지가 같습니다".into()); }
            Some(preview.destination_path)
        } else {
            if row.destination_path.as_deref().is_some_and(|s|!s.trim().is_empty()) { return Err("HOLD에는 목적지를 지정할 수 없습니다".into()); }
            None
        };
        destinations.push(destination);
    }
    Ok(destinations)
}

#[tauri::command]
pub fn import_test_corrections(test_root: String, corrections: Vec<SheetCorrection>) -> Result<Vec<ImportedCorrection>, String> {
    if corrections.is_empty() || corrections.len() > 1000 { return Err("수정 행은 1~1000개만 허용합니다".into()); }
    let mut db = test_database(&test_root)?;
    let destinations = validate_correction_rows(&db, &test_root, &corrections)?;
    let validated = corrections.into_iter().zip(destinations).map(|(row,destination)| {
        let size = row.snapshot_size_bytes.parse::<i64>().expect("validated size");
        let modified = row.snapshot_modified_ns.parse::<i64>().expect("validated timestamp");
        (row,destination,size,modified)
    }).collect::<Vec<_>>();
    let tx = db.transaction().map_err(|e|format!("수정 import transaction 실패: {e}"))?;
    let mut result = Vec::new();
    for (row,destination,size,modified) in validated {
        let prior: Option<(String,String,String,Option<String>)> = tx.query_row("SELECT correction_revision,user_correction,normalized_action,destination_path FROM workspace_corrections WHERE stable_item_id=?1",[&row.stable_item_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
        if let Some((revision,text,action,old_destination)) = prior {
            if revision != row.correction_revision || text != row.user_correction || action != row.normalized_action || old_destination != destination { return Err("기존 수정과 충돌합니다. 자동 덮어쓰기를 중단했습니다".into()); }
        } else {
            tx.execute("INSERT INTO workspace_corrections(stable_item_id,file_id,correction_revision,user_correction,normalized_action,source_path,destination_path,snapshot_size_bytes,snapshot_modified_ns) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![row.stable_item_id,row.file_id,row.correction_revision,row.user_correction,row.normalized_action,row.source_path,destination,size,modified]).map_err(|e|format!("수정 저장 실패: {e}"))?;
        }
        result.push(ImportedCorrection { stable_item_id:row.stable_item_id, file_id:row.file_id, normalized_action:row.normalized_action, source_path:row.source_path, destination_path:destination, status:"imported".into() });
    }
    tx.commit().map_err(|e|format!("수정 import commit 실패: {e}"))?;
    Ok(result)
}

#[tauri::command]
pub fn list_test_corrections(test_root: String) -> Result<Vec<ImportedCorrection>, String> {
    let db = test_database(&test_root)?;
    let mut stmt = db.prepare("SELECT stable_item_id,file_id,normalized_action,source_path,destination_path,status FROM workspace_corrections ORDER BY imported_at,stable_item_id").map_err(|e|e.to_string())?;
    let rows = stmt.query_map([],|r|Ok(ImportedCorrection { stable_item_id:r.get(0)?, file_id:r.get(1)?, normalized_action:r.get(2)?, source_path:r.get(3)?, destination_path:r.get(4)?, status:r.get(5)? })).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDryRun {
    pub stable_item_id: String, pub action: String, pub source_path: String,
    pub destination_path: Option<String>, pub validation_status: String,
    pub expected_change: String, pub undo_possible: bool,
    pub execution_status: String,
}

// Reuse the correction import's full live snapshot/path validation for the proposal. Reimporting
// an identical revision is idempotent. This command never calls the Move executor.
#[tauri::command]
pub fn dry_run_test_correction(test_root: String, stable_item_id: String) -> Result<ActionDryRun, String> {
    let db = test_database(&test_root)?;
    let row: SheetCorrection = db.query_row(
        "SELECT stable_item_id,file_id,correction_revision,user_correction,normalized_action,source_path,destination_path,snapshot_size_bytes,snapshot_modified_ns FROM workspace_corrections WHERE stable_item_id=?1",
        [&stable_item_id], |r| Ok(SheetCorrection {
            stable_item_id:r.get(0)?,file_id:r.get(1)?,correction_revision:r.get(2)?,
            user_correction:r.get(3)?,normalized_action:r.get(4)?,source_path:r.get(5)?,
            destination_path:r.get(6)?,snapshot_size_bytes:r.get::<_,i64>(7)?.to_string(),
            snapshot_modified_ns:r.get::<_,i64>(8)?.to_string(),
        })
    ).map_err(|_|"저장된 수정 제안을 찾을 수 없습니다".to_string())?;
    validate_correction_rows(&db, &test_root, std::slice::from_ref(&row))?;
    // A stored proposal must still match its original revision and data; dry-run is read-only.
    drop(db);
    if row.normalized_action == "MOVE" {
        let target = row.destination_path.clone().ok_or("MOVE 목적지 없음")?;
        let preview = preview_test_correction(test_root,row.file_id,row.user_correction,target)?;
        Ok(ActionDryRun {stable_item_id,action:"MOVE".into(),source_path:preview.source_path,
            destination_path:Some(preview.destination_path),validation_status:"validated_test_root".into(),
            expected_change:"원본 1개를 목적지로 이동하는 제안; 실제 변경 0건".into(),
            undo_possible:true,execution_status:"NOT_EXECUTED".into()})
    } else {
        Ok(ActionDryRun {stable_item_id,action:"HOLD".into(),source_path:row.source_path,
            destination_path:None,validation_status:"validated_test_root".into(),
            expected_change:"파일 변경 없음".into(),undo_possible:false,execution_status:"NOT_EXECUTED".into()})
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchDryRun {
    pub batch_id: String, pub actions: Vec<ActionDryRun>,
    pub validation_status: String, pub execution_status: String,
    pub undo_status: String,
}

#[tauri::command]
pub fn batch_dry_run_test_corrections(test_root: String, batch_id: String, stable_item_ids: Vec<String>) -> Result<BatchDryRun, String> {
    if batch_id.trim().is_empty() || batch_id.len() > 100 || stable_item_ids.is_empty() || stable_item_ids.len() > 100 {
        return Err("Batch ID 또는 제안 개수 제한 위반".into());
    }
    let mut ids = std::collections::HashSet::new();
    let mut sources = std::collections::HashSet::new();
    let mut targets = std::collections::HashSet::new();
    let mut actions = Vec::with_capacity(stable_item_ids.len());
    for id in stable_item_ids {
        if !ids.insert(id.clone()) { return Err("Batch 중복 제안 ID".into()); }
        let action = dry_run_test_correction(test_root.clone(),id)?;
        if action.action == "MOVE" {
            if !sources.insert(action.source_path.clone()) { return Err("Batch 중복 원본".into()); }
            let destination = action.destination_path.as_ref().ok_or("Batch 목적지 없음")?;
            if !targets.insert(crate::windows_target_identity(Path::new(destination))) { return Err("Batch 중복 목적지".into()); }
        }
        actions.push(action);
    }
    if actions.iter().filter_map(|a|a.destination_path.as_ref()).any(|destination|sources.contains(destination)) {
        return Err("Batch 안에서 목적지와 다른 원본이 충돌합니다".into());
    }
    Ok(BatchDryRun {batch_id,actions,validation_status:"DRY_RUN_PASS".into(),
        execution_status:"NOT_EXECUTED".into(),undo_status:"NOT_APPLICABLE".into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn disposable(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fileorbit-{label}-{}-{}",std::process::id(),stamp()))
    }
    #[test]
    fn migrations_reopen_and_reject_future_or_corrupt_db() {
        let root = disposable("migration");
        let path = database_path(&root);
        let db = open_database(&path).unwrap();
        assert_eq!(status(&db).unwrap().schema_version,4);
        drop(db);
        assert_eq!(status(&open_database(&path).unwrap()).unwrap().schema_version,4);
        let db = Connection::open(&path).unwrap();
        db.pragma_update(None,"user_version",99).unwrap();
        drop(db);
        assert!(open_database(&path).err().unwrap().contains("schema version"));
        std::fs::remove_dir_all(&root).unwrap();
        let root = disposable("corrupt");
        let path = database_path(&root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path,b"not a sqlite database").unwrap();
        assert!(open_database(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(),b"not a sqlite database");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn locked_database_returns_error() {
        let root = disposable("locked");
        let path = database_path(&root);
        let db = open_database(&path).unwrap();
        db.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let result = open_database(&path);
        assert!(result.is_err());
        db.execute_batch("ROLLBACK").unwrap();
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn test_root_scan_persists_and_rescans() {
        let root = std::env::temp_dir().join(format!("fileorbit-index-{}-{}",std::process::id(),stamp()));
        let root_str = root.to_string_lossy().into_owned();
        crate::paths::bootstrap_test_root(&root_str,false).unwrap();
        let fixtures = root.join("testdata/fixtures");
        std::fs::create_dir_all(fixtures.join("D-Project/회의록")).unwrap();
        let file = fixtures.join("D-Project/회의록/회의 (1).TXT");
        std::fs::write(&file,b"synthetic").unwrap();
        std::fs::create_dir_all(fixtures.join("node_modules")).unwrap();
        std::fs::write(fixtures.join("node_modules/generated.js"),b"generated").unwrap();
        let scan = fixtures.to_string_lossy().into_owned();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,1);
        let first_seen: String = test_database(&root_str).unwrap().query_row("SELECT last_seen_run_id FROM files",[],|r|r.get(0)).unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,1);
        let unchanged_seen: String = test_database(&root_str).unwrap().query_row("SELECT last_seen_run_id FROM files",[],|r|r.get(0)).unwrap();
        assert_eq!(first_seen,unchanged_seen,"unchanged file should not be rewritten");
        assert_eq!(indexed_files(root_str.clone(),Some("회의".into()),None,Some("txt".into()),None).unwrap().len(),1);
        std::fs::write(&file,b"changed synthetic metadata").unwrap();
        let added = fixtures.join("D-Project/회의록/긴 경로 (2) 보고서.pdf");
        std::fs::write(&added,b"new").unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,2);
        assert_eq!(indexed_files(root_str.clone(),Some("회의".into()),None,Some("txt".into()),None).unwrap()[0].size_bytes,26);
        assert_eq!(test_database(&root_str).unwrap().query_row("SELECT count(*) FROM files",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        let moved = fixtures.join("D-Project/회의록/이동된 회의.txt");
        std::fs::rename(&file,&moved).unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,2);
        assert_eq!(test_database(&root_str).unwrap().query_row("SELECT count(*) FROM files WHERE state='missing'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        std::fs::remove_file(&moved).unwrap();
        std::fs::remove_file(&added).unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan).unwrap().file_count,0);
        assert_eq!(indexed_files(root_str.clone(),None,None,None,None).unwrap()[0].state,"missing");
        assert_eq!(test_database(&root_str).unwrap().query_row("SELECT count(*) FROM files",[],|r|r.get::<_,i64>(0)).unwrap(),3);
        assert!(checked_scan_root(&root_str,&root_str).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn inbox_and_candidates_are_index_only_and_do_not_move_files() {
        let root = disposable("inbox");
        let root_str = root.to_string_lossy().into_owned();
        crate::paths::bootstrap_test_root(&root_str,false).unwrap();
        let fixtures = root.join("testdata");
        let downloads = fixtures.join("Downloads");
        let project = fixtures.join("D-Project/설계");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let source = downloads.join("D-Project 설계 (1).pdf");
        std::fs::write(&source,b"synthetic").unwrap();
        std::fs::write(project.join("D-Project 설계 도면.pdf"),b"synthetic").unwrap();
        scan_indexed_test_root(root_str.clone(),fixtures.to_string_lossy().into_owned()).unwrap();
        let inbox = downloads.to_string_lossy().into_owned();
        assert_eq!(discover_test_inbox(root_str.clone(),inbox.clone()).unwrap().len(),1);
        assert_eq!(discover_test_inbox(root_str.clone(),inbox).unwrap().len(),1);
        let item = &list_test_inbox(root_str.clone()).unwrap()[0];
        assert_eq!(item.review_state,"new");
        assert_eq!(item.action_status,"none");
        assert_eq!(item.index_state,"present");
        assert!(!indexed_candidates(root_str.clone(),item.file_id.clone(),None).unwrap().is_empty());
        let workspace = test_workspace_rows(root_str.clone()).unwrap();
        assert_eq!(workspace[0].stable_item_id,format!("inbox:{}",item.file_id));
        assert_eq!(workspace[0].sync_status,"not_synced");
        let plan = plan_test_workspace_sync(root_str.clone(),vec![]).unwrap();
        assert_eq!(plan.report.inserted,1);
        assert_eq!(plan_test_workspace_sync(root_str.clone(),plan.rows).unwrap().report.unchanged,1);
        let candidates = plan_test_candidate_sync(root_str.clone(),item.file_id.clone(),vec![]).unwrap();
        assert!(candidates.report.inserted > 0);
        assert!(plan_test_candidate_sync(root_str.clone(),item.file_id.clone(),candidates.rows).unwrap().report.unchanged > 0);
        let preview = preview_test_correction(root_str.clone(),item.file_id.clone(),"설계 폴더로".into(),project.join("new.pdf").to_string_lossy().into_owned()).unwrap();
        assert_eq!(preview.dry_run_status,"preview_only");
        assert!(preview_test_correction(root_str.clone(),item.file_id.clone(),"잘못된 대상".into(),source.to_string_lossy().into_owned()).is_err());
        assert!(!project.join("new.pdf").exists());
        assert!(source.exists());
        assert!(checked_scan_root(&root_str,&root_str).is_err());
        drop(list_test_inbox(root_str).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn thousand_file_refresh_preserves_unchanged_rows() {
        let root = disposable("bulk");
        let root_str = root.to_string_lossy().into_owned();
        crate::paths::bootstrap_test_root(&root_str,false).unwrap();
        let fixtures = root.join("testdata/대량 fixture");
        std::fs::create_dir_all(&fixtures).unwrap();
        for i in 0..1200 { std::fs::write(fixtures.join(format!("file-{i}.txt")),b"synthetic").unwrap(); }
        let scan = fixtures.to_string_lossy().into_owned();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,1200);
        let first: String = test_database(&root_str).unwrap().query_row("SELECT last_seen_run_id FROM files WHERE name='file-1.txt'",[],|r|r.get(0)).unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan.clone()).unwrap().file_count,1200);
        let db = test_database(&root_str).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM files WHERE last_seen_run_id=?1",[&first],|r|r.get::<_,i64>(0)).unwrap(),1200);
        drop(db);
        std::fs::write(fixtures.join("file-1.txt"),b"modified synthetic").unwrap();
        std::fs::remove_file(fixtures.join("file-2.txt")).unwrap();
        assert_eq!(scan_indexed_test_root(root_str.clone(),scan).unwrap().file_count,1199);
        let db = test_database(&root_str).unwrap();
        assert_eq!(db.query_row("SELECT count(*) FROM files WHERE last_seen_run_id=?1 AND state='present'",[&first],|r|r.get::<_,i64>(0)).unwrap(),1198);
        assert_eq!(db.query_row("SELECT count(*) FROM files WHERE state='missing'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn scan_does_not_follow_symlink_outside_test_root() {
        let root = disposable("link");
        let outside = disposable("outside");
        let root_str = root.to_string_lossy().into_owned();
        crate::paths::bootstrap_test_root(&root_str,false).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("private.txt"),b"synthetic").unwrap();
        let scan = root.join("testdata/fixtures");
        std::fs::create_dir_all(&scan).unwrap();
        std::os::unix::fs::symlink(&outside,scan.join("escape")).unwrap();
        assert_eq!(scan_indexed_test_root(root_str,scan.to_string_lossy().into_owned()).unwrap().file_count,0);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn correction_import_is_atomic_idempotent_and_preview_only() {
        let root = disposable("correction");
        let root_str = root.to_string_lossy().into_owned();
        crate::paths::bootstrap_test_root(&root_str,false).unwrap();
        let inbox = root.join("testdata/Downloads");
        let destination = root.join("testdata/설계");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        let source = inbox.join("한글 설계.txt");
        std::fs::write(&source,b"synthetic").unwrap();
        discover_test_inbox(root_str.clone(),inbox.to_string_lossy().into_owned()).unwrap();
        let file_id = list_test_inbox(root_str.clone()).unwrap()[0].file_id.clone();
        let db = test_database(&root_str).unwrap();
        let (size,modified): (i64,i64) = db.query_row("SELECT size_bytes,modified_ns FROM files WHERE id=?1",[&file_id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        drop(db);
        let row = SheetCorrection { stable_item_id:format!("inbox:{file_id}"), file_id:file_id.clone(), correction_revision:"rev-1".into(), user_correction:"설계 폴더로".into(), normalized_action:"MOVE".into(), source_path:file_id.clone(), destination_path:Some(destination.join("한글 설계.txt").to_string_lossy().into_owned()), snapshot_size_bytes:size.to_string(), snapshot_modified_ns:modified.to_string() };
        let mut empty = row.clone(); empty.user_correction = "  ".into();
        assert!(import_test_corrections(root_str.clone(),vec![empty]).is_err());
        let mut outside = row.clone(); outside.destination_path = Some(std::env::temp_dir().join("outside.txt").to_string_lossy().into_owned());
        assert!(import_test_corrections(root_str.clone(),vec![outside]).is_err());
        assert!(import_test_corrections(root_str.clone(),vec![row.clone(),row.clone()]).is_err());
        let mut invalid = row.clone(); invalid.file_id = "unknown".into(); invalid.stable_item_id = "inbox:unknown".into();
        assert!(import_test_corrections(root_str.clone(),vec![row.clone(),invalid]).is_err());
        assert_eq!(test_database(&root_str).unwrap().query_row("SELECT count(*) FROM workspace_corrections",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(import_test_corrections(root_str.clone(),vec![row.clone()]).unwrap()[0].status,"imported");
        assert_eq!(list_test_corrections(root_str.clone()).unwrap().len(),1);
        let dry_run = dry_run_test_correction(root_str.clone(),row.stable_item_id.clone()).unwrap();
        assert_eq!(dry_run.execution_status,"NOT_EXECUTED");
        assert!(dry_run.undo_possible);
        let batch=batch_dry_run_test_corrections(root_str.clone(),"synthetic-batch".into(),vec![row.stable_item_id.clone()]).unwrap();
        assert_eq!(batch.actions.len(),1);
        assert_eq!(batch.execution_status,"NOT_EXECUTED");
        assert!(batch_dry_run_test_corrections(root_str.clone(),"duplicate".into(),vec![row.stable_item_id.clone(),row.stable_item_id.clone()]).is_err());
        assert!(source.exists());
        assert!(!destination.join("한글 설계.txt").exists());
        assert_eq!(import_test_corrections(root_str.clone(),vec![row.clone()]).unwrap().len(),1);
        let mut conflicting = row.clone(); conflicting.correction_revision = "rev-2".into();
        assert!(import_test_corrections(root_str.clone(),vec![conflicting]).is_err());
        assert_eq!(test_database(&root_str).unwrap().query_row("SELECT count(*) FROM workspace_corrections",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        std::fs::write(&source,b"changed synthetic").unwrap();
        assert!(import_test_corrections(root_str.clone(),vec![row]).is_err());
        assert!(dry_run_test_correction(root_str.clone(),format!("inbox:{file_id}")).is_err());
        assert!(source.exists());
        assert!(!destination.join("한글 설계.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
