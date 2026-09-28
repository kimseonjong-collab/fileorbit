use std::collections::HashSet;
use serde::{Deserialize, Serialize};

// Provider-neutral review exchange. The filesystem and SQLite remain authoritative.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRow {
    pub stable_item_id: String,
    pub file_id: String,
    pub current_path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub modified_ns: i64,
    pub index_state: String,
    pub review_state: String,
    pub user_correction: String,
    pub action_status: String,
}

pub trait SheetProvider {
    fn inbox_rows(&self) -> Result<Vec<ReviewRow>, String>;
    fn upsert_inbox(&mut self, row: ReviewRow) -> Result<(), String>;
}

#[derive(Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub conflicts: Vec<String>,
}

#[derive(Default)]
pub struct MemorySheet { pub rows: Vec<ReviewRow> }
impl SheetProvider for MemorySheet {
    fn inbox_rows(&self) -> Result<Vec<ReviewRow>, String> { Ok(self.rows.clone()) }
    fn upsert_inbox(&mut self, row: ReviewRow) -> Result<(), String> {
        if let Some(saved) = self.rows.iter_mut().find(|r| r.stable_item_id == row.stable_item_id) { *saved = row; }
        else { self.rows.push(row); }
        Ok(())
    }
}

pub fn sync_inbox<P: SheetProvider>(provider: &mut P, local: &[ReviewRow]) -> Result<SyncReport, String> {
    let remote = provider.inbox_rows()?;
    let mut seen = HashSet::new();
    for row in &remote {
        if row.file_id.is_empty() || row.stable_item_id != format!("inbox:{}", row.file_id) || !seen.insert(&row.stable_item_id) {
            return Err("Sheet 중복 또는 빈 stable item ID: 동기화 중단".into());
        }
    }
    let mut local_seen = HashSet::new();
    for row in local {
        if row.file_id.is_empty() || row.stable_item_id != format!("inbox:{}", row.file_id) || !local_seen.insert(&row.stable_item_id) {
            return Err("SQLite export 중복 또는 빈 stable item ID: 동기화 중단".into());
        }
    }
    let mut report = SyncReport::default();
    for item in local {
        match remote.iter().find(|r| r.stable_item_id == item.stable_item_id) {
            None => {
                provider.upsert_inbox(item.clone())?;
                report.inserted += 1;
            }
            Some(saved) if saved.file_id != item.file_id || saved.current_path != item.current_path => {
                report.conflicts.push(item.stable_item_id.clone());
            }
            Some(saved) if (saved.size_bytes != item.size_bytes || saved.modified_ns != item.modified_ns || saved.index_state != item.index_state)
                && (!saved.user_correction.is_empty() || saved.review_state != "new") => {
                report.conflicts.push(item.stable_item_id.clone());
            }
            Some(saved) => {
                let mut merged = item.clone();
                merged.review_state = saved.review_state.clone();
                merged.user_correction = saved.user_correction.clone();
                merged.action_status = saved.action_status.clone();
                if &merged == saved { report.unchanged += 1; }
                else { provider.upsert_inbox(merged)?; report.updated += 1; }
            }
        }
    }
    Ok(report)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRow {
    pub stable_item_id: String, pub file_id: String, pub candidate_id: String,
    pub candidate_type: String, pub candidate_path: String,
    pub score_basis_points: u16, pub evidence: String,
}

pub fn plan_candidates(local: &[CandidateRow], remote: &[CandidateRow]) -> Result<(SyncReport,Vec<CandidateRow>), String> {
    let mut seen = HashSet::new();
    for row in remote {
        if row.candidate_id.is_empty() || !seen.insert(&row.candidate_id) { return Err("Sheet 후보 ID 중복 또는 누락".into()); }
    }
    seen.clear();
    for row in local {
        if row.candidate_id.is_empty() || !seen.insert(&row.candidate_id) { return Err("Index 후보 ID 중복 또는 누락".into()); }
    }
    let mut merged = remote.to_vec();
    let mut report = SyncReport::default();
    for item in local {
        if item.candidate_id != format!("candidate:{}:{}:{}",item.file_id,item.candidate_type,item.candidate_path)
            || item.stable_item_id != format!("inbox:{}",item.file_id) { return Err("후보 stable ID 불일치".into()); }
        if let Some(saved) = merged.iter_mut().find(|r| r.candidate_id == item.candidate_id) {
            if saved == item { report.unchanged += 1; }
            else { *saved = item.clone(); report.updated += 1; }
        } else { merged.push(item.clone()); report.inserted += 1; }
    }
    Ok((report,merged))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeSheet { rows: Vec<ReviewRow>, fail_write: bool }
    impl SheetProvider for FakeSheet {
        fn inbox_rows(&self) -> Result<Vec<ReviewRow>, String> { Ok(self.rows.clone()) }
        fn upsert_inbox(&mut self, row: ReviewRow) -> Result<(), String> {
            if self.fail_write { return Err("synthetic provider failure".into()); }
            if let Some(saved) = self.rows.iter_mut().find(|r| r.stable_item_id == row.stable_item_id) { *saved = row; }
            else { self.rows.push(row); }
            Ok(())
        }
    }
    fn fixture(id: &str) -> ReviewRow {
        ReviewRow { stable_item_id:format!("inbox:{id}"), file_id:id.into(), current_path:format!("/synthetic/{id}.txt"), filename:format!("{id}.txt"), size_bytes:1, modified_ns:1, index_state:"present".into(), review_state:"new".into(), user_correction:String::new(), action_status:"none".into() }
    }
    #[test]
    fn repeated_export_is_idempotent_and_preserves_review() {
        let mut provider = FakeSheet::default();
        let item = fixture("한글");
        assert_eq!(sync_inbox(&mut provider,&[item.clone()]).unwrap().inserted,1);
        assert_eq!(sync_inbox(&mut provider,&[item.clone()]).unwrap().unchanged,1);
        assert_eq!(provider.rows.len(),1);
        provider.rows[0].user_correction = "보류".into();
        provider.rows[0].review_state = "held".into();
        let mut changed = item;
        changed.size_bytes = 2;
        assert_eq!(sync_inbox(&mut provider,&[changed.clone()]).unwrap().conflicts,vec!["inbox:한글"]);
        assert_eq!(provider.rows[0].user_correction,"보류");
        assert_eq!(provider.rows[0].review_state,"held");
        provider.rows[0].user_correction.clear();
        provider.rows[0].review_state = "new".into();
        assert_eq!(sync_inbox(&mut provider,&[changed]).unwrap().updated,1);
    }
    #[test]
    fn duplicate_and_conflicting_rows_fail_closed() {
        let item = fixture("file");
        let mut provider = FakeSheet { rows:vec![item.clone(),item.clone()], fail_write:false };
        assert!(sync_inbox(&mut provider,&[item.clone()]).is_err());
        provider.rows.pop();
        assert!(sync_inbox(&mut provider,&[item.clone(),item.clone()]).is_err());
        let mut moved = item.clone(); moved.current_path = "/synthetic/other.txt".into();
        let report = sync_inbox(&mut provider,&[moved]).unwrap();
        assert_eq!(report.conflicts,vec![item.stable_item_id]);
        assert_eq!(provider.rows[0].current_path,item.current_path);
        provider.rows.clear(); provider.fail_write = true;
        assert!(sync_inbox(&mut provider,&[fixture("file")]).is_err());
        assert!(provider.rows.is_empty());
    }
    #[test]
    fn candidate_plan_is_idempotent_and_rejects_duplicate_ids() {
        let c = CandidateRow { stable_item_id:"inbox:한글".into(),file_id:"한글".into(),candidate_id:"candidate:한글:folder:/synthetic/설계".into(),candidate_type:"folder".into(),candidate_path:"/synthetic/설계".into(),score_basis_points:5000,evidence:"공통 단어".into() };
        let (first,rows) = plan_candidates(&[c.clone()],&[]).unwrap();
        assert_eq!(first.inserted,1);
        assert_eq!(plan_candidates(&[c.clone()],&rows).unwrap().0.unchanged,1);
        assert!(plan_candidates(&[c.clone()],&[c.clone(),c]).is_err());
    }
    #[test]
    fn malformed_sheet_or_export_identity_fails_before_any_upsert() {
        let good = fixture("good");
        let mut bad = fixture("bad");
        bad.stable_item_id = "inbox:other".into();
        let mut provider = FakeSheet { rows:vec![bad.clone()], fail_write:false };
        assert!(sync_inbox(&mut provider,&[good.clone()]).is_err());
        assert_eq!(provider.rows,vec![bad]);
        provider.rows.clear();
        let mut malformed = fixture("bad");
        malformed.file_id = "other".into();
        assert!(sync_inbox(&mut provider,&[good,malformed]).is_err());
        assert!(provider.rows.is_empty());
    }
}
