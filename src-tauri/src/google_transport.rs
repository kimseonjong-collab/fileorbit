use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::workspace_sync::{MemorySheet, ReviewRow, SyncReport, sync_inbox};

pub const INBOX_TAB: &str = "Inbox_Review";
pub const CANDIDATES_TAB: &str = "Candidates";
pub const CORRECTIONS_TAB: &str = "Corrections";

// An authenticated Google Sheets adapter will implement this boundary. Tokens never enter DTOs,
// SQLite, logs, or the repository. Only machine-owned columns may be written by the adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError { AuthRequired, Network, RemoteConflict, Malformed(String) }

pub trait ReviewTransport {
    fn read_inbox(&mut self) -> Result<Vec<ReviewRow>, TransportError>;
    fn insert_inbox(&mut self, row: &ReviewRow) -> Result<(), TransportError>;
    fn update_owned_inbox(&mut self, row: &ReviewRow) -> Result<(), TransportError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxWireRow {
    pub stable_item_id: String, pub file_id: String, pub current_path: String,
    pub filename: String, pub size_bytes: String, pub modified_ns_utc: String,
    pub index_state: String, pub review_state: String, pub user_correction: String,
    pub action_status: String,
}

impl From<&ReviewRow> for InboxWireRow {
    fn from(row: &ReviewRow) -> Self {
        Self { stable_item_id:row.stable_item_id.clone(), file_id:row.file_id.clone(),
            current_path:row.current_path.clone(), filename:row.filename.clone(),
            size_bytes:row.size_bytes.to_string(), modified_ns_utc:row.modified_ns.to_string(),
            index_state:row.index_state.clone(), review_state:row.review_state.clone(),
            user_correction:row.user_correction.clone(), action_status:row.action_status.clone() }
    }
}

impl TryFrom<InboxWireRow> for ReviewRow {
    type Error = TransportError;
    fn try_from(row: InboxWireRow) -> Result<Self, Self::Error> {
        if row.file_id.is_empty() || row.stable_item_id != format!("inbox:{}", row.file_id) {
            return Err(TransportError::Malformed("stable ID mismatch".into()));
        }
        Ok(Self { stable_item_id:row.stable_item_id, file_id:row.file_id,
            current_path:row.current_path, filename:row.filename,
            size_bytes:row.size_bytes.parse().map_err(|_|TransportError::Malformed("size".into()))?,
            modified_ns:row.modified_ns_utc.parse().map_err(|_|TransportError::Malformed("modified_ns_utc".into()))?,
            index_state:row.index_state, review_state:row.review_state,
            user_correction:row.user_correction, action_status:row.action_status })
    }
}

fn snapshot<T: ReviewTransport>(transport: &mut T) -> Result<Vec<ReviewRow>, TransportError> {
    for attempt in 0..3 {
        match transport.read_inbox() {
            Err(TransportError::Network) if attempt < 2 => continue,
            other => return other,
        }
    }
    unreachable!()
}

fn unique(rows: &[ReviewRow]) -> Result<HashMap<String,ReviewRow>, TransportError> {
    let mut by_id = HashMap::new();
    for row in rows {
        if row.file_id.is_empty() || row.stable_item_id != format!("inbox:{}",row.file_id)
            || by_id.insert(row.stable_item_id.clone(),row.clone()).is_some() {
            return Err(TransportError::RemoteConflict);
        }
    }
    Ok(by_id)
}

// A network error can occur after Google applied a write. Re-read before retrying to avoid
// duplicate rows. A changed remote snapshot fails closed; no filesystem action is called.
pub fn sync_inbox_transport<T: ReviewTransport>(transport: &mut T, local: &[ReviewRow]) -> Result<SyncReport, TransportError> {
    let remote = snapshot(transport)?;
    let previous = unique(&remote)?;
    let mut planned = MemorySheet { rows:remote };
    let report = sync_inbox(&mut planned,local).map_err(TransportError::Malformed)?;
    if !report.conflicts.is_empty() { return Err(TransportError::RemoteConflict); }
    for desired in planned.rows.iter().filter(|r| previous.get(&r.stable_item_id) != Some(*r)) {
        let original = previous.get(&desired.stable_item_id);
        let mut applied = false;
        for attempt in 0..3 {
            let now = unique(&snapshot(transport)?)?;
            match now.get(&desired.stable_item_id) {
                Some(current) if current == desired => { applied = true; break; }
                Some(current) if Some(current) != original => return Err(TransportError::RemoteConflict),
                None if original.is_some() => return Err(TransportError::RemoteConflict),
                _ => {}
            }
            let result = if original.is_some() { transport.update_owned_inbox(desired) }
                else { transport.insert_inbox(desired) };
            match result {
                Ok(()) => { applied = true; break; }
                Err(TransportError::Network) if attempt < 2 => continue,
                Err(e) => return Err(e),
            }
        }
        if !applied { return Err(TransportError::Network); }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fake { rows:Vec<ReviewRow>, fail_after_insert:bool, reads:usize }
    impl ReviewTransport for Fake {
        fn read_inbox(&mut self) -> Result<Vec<ReviewRow>,TransportError> { self.reads+=1; Ok(self.rows.clone()) }
        fn insert_inbox(&mut self,row:&ReviewRow) -> Result<(),TransportError> {
            self.rows.push(row.clone());
            if std::mem::take(&mut self.fail_after_insert) { Err(TransportError::Network) } else { Ok(()) }
        }
        fn update_owned_inbox(&mut self,row:&ReviewRow) -> Result<(),TransportError> {
            let saved=self.rows.iter_mut().find(|r|r.stable_item_id==row.stable_item_id).ok_or(TransportError::RemoteConflict)?;
            saved.current_path=row.current_path.clone(); saved.filename=row.filename.clone();
            saved.size_bytes=row.size_bytes; saved.modified_ns=row.modified_ns;
            saved.index_state=row.index_state.clone(); Ok(())
        }
    }
    fn row(id:&str)->ReviewRow { ReviewRow { stable_item_id:format!("inbox:{id}"),file_id:id.into(),
        current_path:format!("/testdata/{id}.txt"),filename:format!("{id}.txt"),size_bytes:1,
        modified_ns:1,index_state:"present".into(),review_state:"new".into(),
        user_correction:String::new(),action_status:"none".into() } }
    #[test] fn retry_after_ambiguous_insert_does_not_duplicate() {
        let mut fake=Fake { fail_after_insert:true,..Default::default() };
        assert_eq!(sync_inbox_transport(&mut fake,&[row("한글")]).unwrap().inserted,1);
        assert_eq!(fake.rows.len(),1);
        assert_eq!(sync_inbox_transport(&mut fake,&[row("한글")]).unwrap().unchanged,1);
    }
    #[test] fn review_is_preserved_and_stale_metadata_conflicts() {
        let mut fake=Fake { rows:vec![row("a")],..Default::default() };
        fake.rows[0].user_correction="보류".into(); fake.rows[0].review_state="held".into();
        assert_eq!(sync_inbox_transport(&mut fake,&[row("a")]).unwrap().unchanged,1);
        let mut changed=row("a"); changed.size_bytes=2;
        assert_eq!(sync_inbox_transport(&mut fake,&[changed]),Err(TransportError::RemoteConflict));
        assert_eq!(fake.rows[0].user_correction,"보류");
    }
    #[test] fn wire_numbers_are_decimal_text() {
        let original=row("한글"); let wire=InboxWireRow::from(&original);
        assert_eq!(wire.modified_ns_utc,"1");
        assert_eq!(ReviewRow::try_from(wire).unwrap(),original);
    }
}
