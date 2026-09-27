use serde::Serialize;
use std::path::{Path, PathBuf};

const DIRECTORIES: [&str; 9] = [
    "app", "data", "data/backups", "config", "logs", "workspace", "quarantine", "testdata",
    "testdata/fixtures",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRegistry {
    pub root: String,
    pub app: String,
    pub database: String,
    pub backups: String,
    pub config: String,
    pub logs: String,
    pub workspace: String,
    pub quarantine: String,
    pub testdata: String,
    pub planned_directories: Vec<String>,
    pub created_directories: Vec<String>,
    pub dry_run: bool,
}

fn registry(root: &Path, dry_run: bool) -> PathRegistry {
    let path = |name: &str| root.join(name).to_string_lossy().into_owned();
    PathRegistry {
        root: root.to_string_lossy().into_owned(),
        app: path("app"),
        database: path("data/fileorbit.db"),
        backups: path("data/backups"),
        config: path("config"),
        logs: path("logs"),
        workspace: path("workspace"),
        quarantine: path("quarantine"),
        testdata: path("testdata"),
        planned_directories: DIRECTORIES.iter().map(|p| path(p)).collect(),
        created_directories: vec![],
        dry_run,
    }
}

fn safe_test_root(value: &str) -> Result<PathBuf, String> {
    if value.trim().is_empty() { return Err("Test Root가 비어 있습니다".into()); }
    let root = PathBuf::from(value);
    if !root.is_absolute() || root.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("Test Root는 상위 이동 요소가 없는 절대 경로여야 합니다".into());
    }
    let canonical_parent = root.parent().ok_or("Test Root의 상위 폴더가 없습니다")?
        .canonicalize().map_err(|e| format!("Test Root 상위 폴더 확인 실패: {e}"))?;
    if !canonical_parent.is_dir() { return Err("Test Root 상위 경로가 폴더가 아닙니다".into()); }
    let leaf = root.file_name().ok_or("Test Root 이름이 없습니다")?;
    let root = canonical_parent.join(leaf);
    if root.exists() {
        let metadata = std::fs::symlink_metadata(&root).map_err(|e| e.to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Test Root는 일반 폴더여야 합니다".into());
        }
    }
    #[cfg(windows)]
    {
        let normalized = root.to_string_lossy().trim_start_matches(r"\\?\").to_ascii_lowercase();
        if normalized == r"c:\fileorbit" || normalized.starts_with(r"c:\fileorbit\") {
            return Err("실제 표준 경로와 그 하위 폴더는 Test Root로 사용할 수 없습니다".into());
        }
    }
    Ok(root)
}

pub fn bootstrap_test_root(value: &str, dry_run: bool) -> Result<PathRegistry, String> {
    let root = safe_test_root(value)?;
    let mut result = registry(&root, dry_run);
    if dry_run { return Ok(result); }
    std::fs::create_dir(&root).or_else(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists && root.is_dir() { Ok(()) } else { Err(e) }
    }).map_err(|e| format!("Test Root 생성 실패: {e}"))?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    for relative in DIRECTORIES {
        let mut cursor = root.clone();
        for part in relative.split('/') {
            cursor.push(part);
            if cursor.exists() {
                let md = std::fs::symlink_metadata(&cursor).map_err(|e| e.to_string())?;
                if md.file_type().is_symlink() || !md.is_dir() { return Err(format!("폴더가 아닌 경로: {}", cursor.display())); }
            } else {
                std::fs::create_dir(&cursor).map_err(|e| e.to_string())?;
                result.created_directories.push(cursor.to_string_lossy().into_owned());
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn bootstrap_paths(test_root: String, dry_run: bool) -> Result<PathRegistry, String> {
    bootstrap_test_root(&test_root, dry_run)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dry_run_creates_nothing_and_bootstrap_is_idempotent() {
        let parent = std::env::temp_dir();
        let root = parent.join(format!("fileorbit-path-test-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let name = root.to_string_lossy();
        let preview = bootstrap_test_root(&name, true).unwrap();
        assert_eq!(preview.planned_directories.len(), 9);
        assert!(!root.exists());
        let created = bootstrap_test_root(&name, false).unwrap();
        assert!(Path::new(&created.database).parent().unwrap().is_dir());
        assert!(!Path::new(&created.database).exists());
        assert_eq!(created.created_directories.len(), 9);
        assert!(bootstrap_test_root(&name, false).unwrap().created_directories.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rejects_relative_and_parent_traversal() {
        assert!(bootstrap_test_root("relative", true).is_err());
        let value = std::env::temp_dir().join("..").join("unsafe");
        assert!(bootstrap_test_root(&value.to_string_lossy(), true).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn rejects_actual_standard_root_with_case_variation() {
        assert!(bootstrap_test_root(r"C:\FileOrbit", true).is_err());
        assert!(bootstrap_test_root(r"c:\fileorbit", true).is_err());
        assert!(bootstrap_test_root(r"C:\FileOrbit\testdata", true).is_err());
    }
}
