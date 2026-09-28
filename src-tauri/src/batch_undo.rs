//! Pure batch journal model. No filesystem operation or production executor is reachable here.
use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchActionRecord {
    pub batch_id: String,
    pub action_id: String,
    pub sequence: usize,
    pub action_type: String,
    pub source_before: String,
    pub destination_after: String,
    pub source_snapshot: String,
    pub execution_result: String,
    pub verification_result: String,
    pub undo_state: String,
    pub reverse_order: Option<usize>,
    pub created_at: String,
    pub executed_at: Option<String>,
}

/// A failed or unverified action cannot enter the reverse plan. The caller must verify
/// each destination against its recorded snapshot before any future executor is used.
pub fn reverse_plan(records: &[BatchActionRecord]) -> Result<Vec<BatchActionRecord>, String> {
    if records.is_empty() || records.len() > 100 { return Err("Batch 크기 제한".into()); }
    let batch = &records[0].batch_id;
    if batch.is_empty() { return Err("Batch ID 없음".into()); }
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();
    let mut targets = HashSet::new();
    for (i, record) in records.iter().enumerate() {
        if record.batch_id != *batch || record.sequence != i || record.action_id.is_empty()
            || !ids.insert(&record.action_id) || record.action_type != "MOVE"
            || record.source_before.is_empty() || record.destination_after.is_empty()
            || record.source_before == record.destination_after || record.source_snapshot.is_empty()
            || record.created_at.is_empty() || record.reverse_order.is_some() {
            return Err("Batch journal 구조 또는 중복 action 오류".into());
        }
        if !sources.insert(&record.source_before) || !targets.insert(&record.destination_after) {
            return Err("Batch 중복 원본/목적지".into());
        }
    }
    if targets.iter().any(|target| sources.contains(target)) { return Err("Batch 경로 체인 충돌".into()); }
    let mut result = Vec::new();
    for record in records.iter().rev() {
        match record.execution_result.as_str() {
            "NOT_EXECUTED" | "FAILED" => continue,
            "MOVED" if record.verification_result == "VERIFIED" && record.undo_state == "ELIGIBLE"
                && record.executed_at.is_some() => {
                let mut planned = record.clone();
                planned.reverse_order = Some(result.len());
                result.push(planned);
            }
            _ => return Err("실행·검증·Undo 상태 불일치: 재확인 필요".into()),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(i: usize) -> BatchActionRecord { BatchActionRecord {
        batch_id:"fixture".into(),action_id:format!("a{i}"),sequence:i,action_type:"MOVE".into(),
        source_before:format!("/testdata/source{i}"),destination_after:format!("/testdata/target{i}"),
        source_snapshot:"size=1;modified=1".into(),execution_result:"MOVED".into(),
        verification_result:"VERIFIED".into(),undo_state:"ELIGIBLE".into(),
        reverse_order:None,created_at:"t0".into(),executed_at:Some("t1".into()) } }
    #[test] fn success_and_partial_failure_reverse_only_verified_moves() {
        let mut rows = vec![row(0),row(1),row(2)];
        assert_eq!(reverse_plan(&rows).unwrap().iter().map(|r|r.action_id.as_str()).collect::<Vec<_>>(),vec!["a2","a1","a0"]);
        for failed in 0..3 {
            rows[failed].execution_result="FAILED".into();
            rows[failed].verification_result="NOT_APPLICABLE".into();
            rows[failed].undo_state="NOT_APPLICABLE".into();
            assert_eq!(reverse_plan(&rows).unwrap().len(),2);
            rows[failed]=row(failed);
        }
    }
    #[test] fn conflicts_and_changed_destination_fail_closed() {
        let mut rows=vec![row(0),row(1)];
        rows[1].action_id="a0".into(); assert!(reverse_plan(&rows).is_err()); rows[1]=row(1);
        rows[1].source_before=rows[0].destination_after.clone(); assert!(reverse_plan(&rows).is_err()); rows[1]=row(1);
        rows[1].verification_result="CHANGED".into(); assert!(reverse_plan(&rows).is_err()); rows[1]=row(1);
        rows[1].undo_state="UNDONE".into(); assert!(reverse_plan(&rows).is_err());
    }
}
