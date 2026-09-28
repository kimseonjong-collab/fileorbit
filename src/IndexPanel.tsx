import {useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";

type Status={schemaVersion:number;fileCount:number;lastScan:string|null};
type FileRow={path:string;name:string;extension:string|null;sizeBytes:number;state:string};
type InboxRow={fileId:string;filename:string;currentPath:string;extension:string|null;sizeBytes:number;modifiedNsText:string;indexState:string;reviewState:string;relatedState:string;proposedDestination:string|null;reason:string|null;confidence:number|null;actionStatus:string};
type Candidate={candidate:string;candidateType:string;score:number;evidence:string};
type Correction={stableItemId:string;fileId:string;normalizedAction:string;sourcePath:string;destinationPath:string|null;status:string};
type Preview={sourcePath:string;destinationPath:string;validationStatus:string;dryRunStatus:string};
type SyncReport={inserted:number;updated:number;unchanged:number;conflicts:string[]};
type SyncPlan={report:SyncReport;rows:unknown[]};
type ActionDryRun={stableItemId:string;action:string;sourcePath:string;destinationPath:string|null;validationStatus:string;expectedChange:string;undoPossible:boolean;executionStatus:string};
type BatchDryRun={batchId:string;actions:ActionDryRun[];validationStatus:string;executionStatus:string;undoStatus:string;undoEligibleCount:number;reverseOrder:string[];verificationState:string};
type NaturalIR={referencedItemIds:string[];action:string;destinationReference:string|null;status:string;requiresSafetyValidation:boolean;requiresExplicitApproval:boolean};
type DestinationResolution={status:"RESOLVED"|"AMBIGUOUS"|"NOT_FOUND"|"NOT_ALLOWED";folderId:string|null;destinationPath:string|null;matchedCount:number;reason:string};

export function IndexPanel({isWeb}:{isWeb:boolean}){
  const [appStatus,setAppStatus]=useState<Status|null>(null);
  const [testRoot,setTestRoot]=useState("");
  const [scanRoot,setScanRoot]=useState("");
  const [status,setStatus]=useState<Status|null>(null);
  const [rows,setRows]=useState<FileRow[]>([]);
  const [inbox,setInbox]=useState<InboxRow[]>([]);
  const [candidates,setCandidates]=useState<Candidate[]>([]);
  const [corrections,setCorrections]=useState<Correction[]>([]);
  const [selectedId,setSelectedId]=useState("");
  const [correctionText,setCorrectionText]=useState("");
  const [destination,setDestination]=useState("");
  const [decision,setDecision]=useState<"MOVE"|"HOLD">("HOLD");
  const [revision,setRevision]=useState("");
  const [naturalText,setNaturalText]=useState("");
  const [naturalIR,setNaturalIR]=useState<NaturalIR|null>(null);
  const [destinationResolution,setDestinationResolution]=useState<DestinationResolution|null>(null);
  const [preview,setPreview]=useState<Preview|null>(null);
  const [inboxSync,setInboxSync]=useState<SyncReport|null>(null);
  const [candidateSync,setCandidateSync]=useState<SyncReport|null>(null);
  const [actionDryRun,setActionDryRun]=useState<ActionDryRun|null>(null);
  const [batchDryRun,setBatchDryRun]=useState<BatchDryRun|null>(null);
  const [error,setError]=useState("");
  const [busy,setBusy]=useState(false);
  useEffect(()=>{if(!isWeb)invoke<Status>("app_index_status").then(setAppStatus).catch(()=>setError("로컬 Index 연결 상태를 확인할 수 없습니다."))},[isWeb]);
  async function load(){
    setBusy(true);setError("");
    try{
      setStatus(await invoke<Status>("index_status",{testRoot}));
      setRows(await invoke<FileRow[]>("indexed_files",{testRoot,limit:20}));
      setCorrections(await invoke<Correction[]>("list_test_corrections",{testRoot}));
    }catch{setError("Test Root DB를 조회할 수 없습니다. 경로와 권한을 확인하십시오.")}
    finally{setBusy(false)}
  }
  async function scan(){
    setBusy(true);setError("");
    try{
      const next=await invoke<Status>("scan_indexed_test_root",{testRoot,scanRoot});
      setStatus(next);
      setRows(await invoke<FileRow[]>("indexed_files",{testRoot,limit:20}));
    }catch{setError("Test Root 경로 또는 DB 스캔을 확인하십시오. 기존 파일은 변경되지 않았습니다.")}
    finally{setBusy(false)}
  }
  async function discoverInbox(){
    setBusy(true);setError("");setCandidates([]);
    try{setInbox(await invoke<InboxRow[]>("discover_test_inbox",{testRoot,inboxPath:scanRoot}));setCorrections(await invoke<Correction[]>("list_test_corrections",{testRoot}))}
    catch{setError("Test Root/testdata 내부 Inbox 폴더만 허용됩니다. 파일 이동은 수행되지 않았습니다.")}
    finally{setBusy(false)}
  }
  async function showCandidates(fileId:string){
    setBusy(true);setError("");
    try{
      setCandidates(await invoke<Candidate[]>("indexed_candidates",{testRoot,fileId,limit:5}));
      setCandidateSync((await invoke<SyncPlan>("plan_test_candidate_sync",{testRoot,fileId,remoteRows:[]})).report);
    }
    catch{setError("저장된 Index 후보를 조회할 수 없습니다.")}
    finally{setBusy(false)}
  }
  async function previewWorkspaceExport(){
    setBusy(true);setError("");
    try{setInboxSync((await invoke<SyncPlan>("plan_test_workspace_sync",{testRoot,remoteRows:[]})).report)}
    catch{setError("Test Root Workspace 계획을 계산할 수 없습니다. Google 전송은 실행되지 않았습니다.")}
    finally{setBusy(false)}
  }
  async function dryRunCorrection(stableItemId:string){
    setBusy(true);setError("");setActionDryRun(null);
    try{setActionDryRun(await invoke<ActionDryRun>("dry_run_test_correction",{testRoot,stableItemId}))}
    catch{setError("원본 상태 또는 목적지가 저장 시점과 달라 Dry-run을 중단했습니다. 다시 스캔·검토하십시오.")}
    finally{setBusy(false)}
  }
  async function dryRunBatch(){
    setBusy(true);setError("");setBatchDryRun(null);
    try{setBatchDryRun(await invoke<BatchDryRun>("batch_dry_run_test_corrections",{testRoot,batchId:`preview-${Date.now()}`,stableItemIds:corrections.map(c=>c.stableItemId)}))}
    catch{setError("Batch 중 중복·충돌·변경된 원본이 있어 계획을 중단했습니다. 파일 작업은 없었습니다.")}
    finally{setBusy(false)}
  }
  async function saveCorrection(){
    const item=inbox.find(r=>r.fileId===selectedId);
    if(!item)return;
    setBusy(true);setError("");setPreview(null);
    try{
      const checked=decision==="MOVE"?await invoke<Preview>("preview_test_correction",{testRoot,fileId:item.fileId,userCorrection:correctionText,destinationPath:destination}):null;
      const imported=await invoke<Correction[]>("import_test_corrections",{testRoot,corrections:[{stableItemId:`inbox:${item.fileId}`,fileId:item.fileId,correctionRevision:revision,userCorrection:correctionText,normalizedAction:decision,sourcePath:item.currentPath,destinationPath:decision==="MOVE"?destination:null,snapshotSizeBytes:String(item.sizeBytes),snapshotModifiedNs:item.modifiedNsText}]});
      setCorrections(await invoke<Correction[]>("list_test_corrections",{testRoot}));
      if(imported[0]?.normalizedAction==="MOVE")setPreview(checked);
    }catch{setError("수정 제안이 거부되었습니다. 중복·경로 충돌·파일 변경 여부를 재확인하십시오. 파일 작업은 수행되지 않았습니다.")}
    finally{setBusy(false)}
  }
  async function interpretCorrection(){
    setBusy(true);setError("");setNaturalIR(null);setDestinationResolution(null);
    try{
      const ir=await invoke<NaturalIR>("parse_test_correction",{text:naturalText,selectedItemIds:[`inbox:${selectedId}`]});
      setNaturalIR(ir);
      if(ir.action==="MOVE"&&ir.destinationReference){
        setDestinationResolution(await invoke<DestinationResolution>("resolve_test_destination",{testRoot,fileId:selectedId,reference:ir.destinationReference}));
      }
    }
    catch{setError("문장 형식이 불명확합니다. 보류, 제외, 이동: <목적지> 중 하나로 입력하십시오. 저장·실행은 없었습니다.")}
    finally{setBusy(false)}
  }
  return <section className="panel" aria-label="로컬 SQLite Index">
    <div className="panelHead"><div><h3>로컬 Index · Test Root</h3><p>합성 테스트 폴더만 저장·조회합니다. 기존 정리 기능과 별도로 동작합니다.</p></div><span>{isWeb?"데스크톱 앱 전용":appStatus?`DB 연결 · schema v${appStatus.schemaVersion}`:"DB 확인 중"}</span></div>
    <h4>1 Observe · Index와 Inbox</h4>
    {!isWeb&&<div style={{display:"flex",gap:8,flexWrap:"wrap",alignItems:"center"}}>
      <input aria-label="Test Root" placeholder="Test Root 절대 경로" value={testRoot} onChange={e=>{setTestRoot(e.target.value);setInboxSync(null);setCandidateSync(null);setSelectedId("")}} style={{minWidth:240}}/>
      <input aria-label="스캔 폴더" placeholder="Test Root/testdata 내부 경로" value={scanRoot} onChange={e=>setScanRoot(e.target.value)} style={{minWidth:260}}/>
      <button disabled={busy||!testRoot||!scanRoot} onClick={scan}>{busy?"스캔 중":"테스트 Index 스캔"}</button>
      <button disabled={busy||!testRoot||!scanRoot} onClick={discoverInbox}>테스트 Inbox 후보 조회</button>
      <button disabled={busy||!testRoot} onClick={load}>저장된 Index 조회</button>
      <button disabled={busy||!testRoot} onClick={previewWorkspaceExport}>Workspace 내보내기 계획</button>
    </div>}
    {status&&<p>인덱스 파일 {status.fileCount}개 · 마지막 완료 {status.lastScan??"없음"}</p>}
    {rows.length>0&&<div style={{maxHeight:150,overflow:"auto"}}>{rows.map(r=><div key={r.path}>{r.state==="present"?"●":"○"} {r.name} · {r.path}</div>)}</div>}
    {inboxSync&&<p>빈 Sheet 기준 Inbox 내보내기 계획: 신규 {inboxSync.inserted} · 갱신 {inboxSync.updated} · 동일 {inboxSync.unchanged} · 충돌 {inboxSync.conflicts.length}. 실제 Sheet 내용과 비교하거나 전송하지 않았습니다.</p>}
    <h4>2 Review · 관련 후보와 근거</h4>
    {inbox.length>0&&<div style={{maxHeight:180,overflow:"auto"}}><strong>Test Inbox · 검토 후보 {inbox.length}개</strong>{inbox.map(r=><div key={r.fileId}>{r.filename} · {r.extension??"유형 없음"} · {r.sizeBytes} bytes · {r.reviewState}/{r.relatedState}/{r.indexState}/{r.actionStatus} · {r.currentPath}{r.proposedDestination?` → ${r.proposedDestination} (${r.reason??"근거 없음"}, ${Math.round((r.confidence??0)*100)}%)`:""} <button disabled={busy||r.indexState!=="present"} onClick={()=>{setSelectedId(r.fileId);setRevision(`ui-${Date.now()}`);setPreview(null);showCandidates(r.fileId)}}>관련 후보·선택</button></div>)}</div>}
    {candidates.length>0&&<div style={{maxHeight:120,overflow:"auto"}}>{candidates.map(r=><div key={r.candidate}>{r.candidateType} · {r.candidate} · {Math.round(r.score*100)}% · {r.evidence}</div>)}</div>}
    {candidateSync&&<p>빈 Sheet 기준 선택 파일 후보 계획: 신규 {candidateSync.inserted} · 갱신 {candidateSync.updated} · 충돌 {candidateSync.conflicts.length}. 실제 Sheet 내용과 비교하거나 전송하지 않았습니다.</p>}
    <h4>3 Decide · 수정 제안 또는 보류</h4>
    {!isWeb&&selectedId&&<div style={{display:"flex",gap:8,flexWrap:"wrap"}}>
      <select aria-label="수정 action" value={decision} onChange={e=>setDecision(e.target.value as "MOVE"|"HOLD")}><option value="HOLD">보류</option><option value="MOVE">이동 제안</option></select>
      <input aria-label="수정 설명" placeholder="수정 이유 또는 자연어 설명" value={correctionText} onChange={e=>setCorrectionText(e.target.value)}/>
      {decision==="MOVE"&&<input aria-label="제안 목적지" placeholder="Test Root/testdata 내부 목적지 절대 경로" value={destination} onChange={e=>setDestination(e.target.value)} style={{minWidth:280}}/>}
      <button disabled={busy||!correctionText.trim()||(decision==="MOVE"&&!destination.trim())} onClick={saveCorrection}>제안 저장·안전검증</button>
    </div>}
    {!isWeb&&selectedId&&<div style={{display:"flex",gap:8,flexWrap:"wrap"}}>
      <input aria-label="제한된 자연어 수정" placeholder="보류 / 제외 / 이동: <폴더 ID 또는 정확한 폴더명>" value={naturalText} onChange={e=>{setNaturalText(e.target.value);setNaturalIR(null);setDestinationResolution(null)}}/>
      <button disabled={busy||!naturalText.trim()} onClick={interpretCorrection}>문장 해석 미리보기</button>
      {naturalIR&&<p>{naturalIR.action} · {naturalIR.destinationReference??"목적지 없음"} · {naturalIR.status} · 별도 안전검증·명시적 승인 필요 · 저장·실행 안 함</p>}
      {destinationResolution&&<p role="status">목적지 참조 {destinationResolution.status} · {destinationResolution.folderId??"폴더 확정 없음"} · {destinationResolution.matchedCount}건 · {destinationResolution.reason} · {destinationResolution.destinationPath??"경로 없음"}</p>}
    </div>}
    <h4>4 Execute · Dry-run</h4>
    <p>{preview?`${preview.sourcePath} → ${preview.destinationPath} · ${preview.validationStatus} · ${preview.dryRunStatus}`:"실행 전용 버튼 없음 · Test Root 제안만 검증"}</p>
    {actionDryRun&&<p>재검증 {actionDryRun.action} · {actionDryRun.validationStatus} · {actionDryRun.expectedChange} · Undo 가능 {actionDryRun.undoPossible?"예":"아니오"} · {actionDryRun.executionStatus}</p>}
    {corrections.length>0&&<button disabled={busy||corrections.length>100} onClick={dryRunBatch}>저장 제안 전체 Batch Dry-run</button>}
    {batchDryRun&&<div role="status">Batch {batchDryRun.batchId} · {batchDryRun.actions.length}건 · {batchDryRun.validationStatus} · {batchDryRun.executionStatus} · 검증 {batchDryRun.verificationState} · Undo {batchDryRun.undoStatus} · 대상 {batchDryRun.undoEligibleCount}건 · 역순 {batchDryRun.reverseOrder.length?batchDryRun.reverseOrder.join(" → "):"없음 (실행 전)"}
      <ol>{batchDryRun.actions.map((a,i)=><li key={a.stableItemId}>{i+1}. {a.action} · {a.stableItemId} · {a.validationStatus} · {a.executionStatus} · Undo {a.executionStatus==="NOT_EXECUTED"?"NOT_APPLICABLE":"재검증 필요"}</li>)}</ol>
    </div>}
    <h4>5 Verify / Undo · 기록</h4>
    {corrections.length>0?<div>{corrections.map(c=><div key={c.stableItemId}>{c.normalizedAction} · {c.status} · {c.sourcePath}{c.destinationPath?` → ${c.destinationPath}`:""} · 실제 실행 없음 <button disabled={busy} onClick={()=>dryRunCorrection(c.stableItemId)}>저장 제안 Dry-run 재검증</button></div>)}</div>:<p>실행·Undo 기록 없음</p>}
    {error&&<p role="alert">{error}</p>}
  </section>
}
