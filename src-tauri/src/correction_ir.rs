use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// Intent is supplied by an interpreter or a user control, never inferred from text here.
// This representation cannot be passed to a filesystem executor.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionInput {
    pub user_text: String,
    pub referenced_item_ids: Vec<String>,
    pub intent: String,
    pub destination_reference: Option<String>,
    pub ambiguous: bool,
    pub confidence_basis_points: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedCorrection {
    pub referenced_item_ids: Vec<String>,
    pub action: String,
    pub destination_reference: Option<String>,
    pub status: String,
    pub requires_safety_validation: bool,
    pub requires_explicit_approval: bool,
}

pub fn normalize(input: CorrectionInput) -> Result<NormalizedCorrection,String> {
    if input.user_text.trim().is_empty() || input.referenced_item_ids.is_empty()
        || input.referenced_item_ids.len() > 100 || input.ambiguous
        || input.confidence_basis_points > 10_000 {
        return Err("자연어 수정의 대상·의도 또는 신뢰도가 불명확합니다".into());
    }
    let mut seen=HashSet::new();
    for id in &input.referenced_item_ids {
        if !id.starts_with("inbox:") || id.len() <= 6 || !seen.insert(id) {
            return Err("대상 ID가 잘못되었거나 중복됐습니다".into());
        }
    }
    let destination=input.destination_reference.as_deref().map(str::trim).filter(|s|!s.is_empty());
    match input.intent.as_str() {
        "MOVE" => {
            let target=destination.ok_or("MOVE 목적지 참조가 필요합니다")?;
            Ok(NormalizedCorrection {referenced_item_ids:input.referenced_item_ids,
                action:"MOVE".into(),destination_reference:Some(target.into()),
                status:"NEEDS_SAFETY_VALIDATION".into(),requires_safety_validation:true,
                requires_explicit_approval:true})
        }
        "HOLD" | "REJECT" if destination.is_none() => Ok(NormalizedCorrection {
            referenced_item_ids:input.referenced_item_ids,action:input.intent,
            destination_reference:None,status:"PROPOSAL_ONLY".into(),
            requires_safety_validation:false,requires_explicit_approval:true}),
        _ => Err("지원하지 않는 의도 또는 불필요한 목적지입니다".into()),
    }
}

// Deliberately narrow grammar. Free-form prose cannot authorize a path or an action.
// The selected IDs come from the Index UI, never from text supplied by a Sheet.
#[tauri::command]
pub fn parse_test_correction(text: String, selected_item_ids: Vec<String>) -> Result<NormalizedCorrection,String> {
    let trimmed = text.trim();
    let (intent, destination) = match trimmed {
        "보류" | "HOLD" => ("HOLD", None),
        "제외" | "REJECT" => ("REJECT", None),
        _ => {
            let value = trimmed.strip_prefix("이동: ").or_else(||trimmed.strip_prefix("MOVE: "))
                .ok_or("지원하는 형식은 보류, 제외, 이동: <목적지>입니다")?;
            if value.trim() != value || value.is_empty() || value.contains('\n') || value.contains('\r') {
                return Err("목적지 참조 형식이 불명확합니다".into());
            }
            ("MOVE",Some(value.to_owned()))
        }
    };
    normalize(CorrectionInput { user_text:trimmed.into(), referenced_item_ids:selected_item_ids,
        intent:intent.into(), destination_reference:destination, ambiguous:false,
        confidence_basis_points:10_000 })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(intent:&str)->CorrectionInput {CorrectionInput {user_text:"이 파일은 설계 자료".into(),
        referenced_item_ids:vec!["inbox:한글".into()],intent:intent.into(),
        destination_reference:None,ambiguous:false,confidence_basis_points:7500}}
    #[test] fn move_remains_a_proposal_requiring_safety_and_approval() {
        let mut raw=input("MOVE");
        assert!(normalize(raw.clone()).is_err());
        raw.destination_reference=Some("설계 폴더".into());
        let result=normalize(raw).unwrap();
        assert_eq!(result.status,"NEEDS_SAFETY_VALIDATION");
        assert!(result.requires_explicit_approval);
    }
    #[test] fn ambiguous_duplicate_or_unsupported_intent_is_rejected() {
        let mut raw=input("HOLD"); raw.ambiguous=true; assert!(normalize(raw).is_err());
        let mut raw=input("HOLD"); raw.referenced_item_ids.push("inbox:한글".into()); assert!(normalize(raw).is_err());
        assert!(normalize(input("DELETE")).is_err());
        assert_eq!(normalize(input("HOLD")).unwrap().status,"PROPOSAL_ONLY");
    }
    #[test] fn deterministic_text_only_produces_unapproved_ir() {
        let ids=vec!["inbox:한글".into()];
        let move_ir=parse_test_correction("이동: /testdata/설계/자료.txt".into(),ids.clone()).unwrap();
        assert_eq!(move_ir.action,"MOVE");
        assert!(move_ir.requires_safety_validation && move_ir.requires_explicit_approval);
        assert_eq!(parse_test_correction("보류".into(),ids.clone()).unwrap().action,"HOLD");
        assert_eq!(parse_test_correction("제외".into(),ids.clone()).unwrap().action,"REJECT");
        for prose in ["이 파일을 옮겨줘", "이동: /a\n삭제: /b", "이동: ", "이동:/a"] {
            assert!(parse_test_correction(prose.into(),ids.clone()).is_err());
        }
        assert!(parse_test_correction("보류".into(),vec![]).is_err());
    }
}
