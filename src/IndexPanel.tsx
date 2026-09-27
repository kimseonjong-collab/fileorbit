import {useEffect,useState} from "react";
import {invoke} from "@tauri-apps/api/core";

type Status={schemaVersion:number;fileCount:number;lastScan:string|null};
type FileRow={path:string;name:string;extension:string|null;sizeBytes:number;state:string};
type InboxRow={fileId:string;filename:string;currentPath:string;extension:string|null;sizeBytes:number;modifiedNs:number;indexState:string;reviewState:string;relatedState:string;proposedDestination:string|null;reason:string|null;confidence:number|null;actionStatus:string};
type Candidate={candidate:string;candidateType:string;score:number;evidence:string};

export function IndexPanel({isWeb}:{isWeb:boolean}){
  const [appStatus,setAppStatus]=useState<Status|null>(null);
  const [testRoot,setTestRoot]=useState("");
  const [scanRoot,setScanRoot]=useState("");
  const [status,setStatus]=useState<Status|null>(null);
  const [rows,setRows]=useState<FileRow[]>([]);
  const [inbox,setInbox]=useState<InboxRow[]>([]);
  const [candidates,setCandidates]=useState<Candidate[]>([]);
  const [error,setError]=useState("");
  const [busy,setBusy]=useState(false);
  useEffect(()=>{if(!isWeb)invoke<Status>("app_index_status").then(setAppStatus).catch(()=>setError("로컬 Index 연결 상태를 확인할 수 없습니다."))},[isWeb]);
  async function load(){
    setBusy(true);setError("");
    try{
      setStatus(await invoke<Status>("index_status",{testRoot}));
      setRows(await invoke<FileRow[]>("indexed_files",{testRoot,limit:20}));
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
    try{setInbox(await invoke<InboxRow[]>("discover_test_inbox",{testRoot,inboxPath:scanRoot}))}
    catch{setError("Test Root/testdata 내부 Inbox 폴더만 허용됩니다. 파일 이동은 수행되지 않았습니다.")}
    finally{setBusy(false)}
  }
  async function showCandidates(fileId:string){
    setBusy(true);setError("");
    try{setCandidates(await invoke<Candidate[]>("indexed_candidates",{testRoot,fileId,limit:5}))}
    catch{setError("저장된 Index 후보를 조회할 수 없습니다.")}
    finally{setBusy(false)}
  }
  return <section className="panel" aria-label="로컬 SQLite Index">
    <div className="panelHead"><div><h3>로컬 Index · Test Root</h3><p>합성 테스트 폴더만 저장·조회합니다. 기존 정리 기능과 별도로 동작합니다.</p></div><span>{isWeb?"데스크톱 앱 전용":appStatus?`DB 연결 · schema v${appStatus.schemaVersion}`:"DB 확인 중"}</span></div>
    {!isWeb&&<div style={{display:"flex",gap:8,flexWrap:"wrap",alignItems:"center"}}>
      <input aria-label="Test Root" placeholder="Test Root 절대 경로" value={testRoot} onChange={e=>setTestRoot(e.target.value)} style={{minWidth:240}}/>
      <input aria-label="스캔 폴더" placeholder="Test Root/testdata 내부 경로" value={scanRoot} onChange={e=>setScanRoot(e.target.value)} style={{minWidth:260}}/>
      <button disabled={busy||!testRoot||!scanRoot} onClick={scan}>{busy?"스캔 중":"테스트 Index 스캔"}</button>
      <button disabled={busy||!testRoot||!scanRoot} onClick={discoverInbox}>테스트 Inbox 후보 조회</button>
      <button disabled={busy||!testRoot} onClick={load}>저장된 Index 조회</button>
    </div>}
    {status&&<p>인덱스 파일 {status.fileCount}개 · 마지막 완료 {status.lastScan??"없음"}</p>}
    {rows.length>0&&<div style={{maxHeight:150,overflow:"auto"}}>{rows.map(r=><div key={r.path}>{r.state==="present"?"●":"○"} {r.name} · {r.path}</div>)}</div>}
    {inbox.length>0&&<div style={{maxHeight:180,overflow:"auto"}}><strong>Test Inbox · 검토 후보 {inbox.length}개</strong>{inbox.map(r=><div key={r.fileId}>{r.filename} · {r.extension??"유형 없음"} · {r.sizeBytes} bytes · {r.reviewState}/{r.relatedState}/{r.indexState}/{r.actionStatus} · {r.currentPath}{r.proposedDestination?` → ${r.proposedDestination} (${r.reason??"근거 없음"}, ${Math.round((r.confidence??0)*100)}%)`:""} <button disabled={busy||r.indexState!=="present"} onClick={()=>showCandidates(r.fileId)}>관련 후보</button></div>)}</div>}
    {candidates.length>0&&<div style={{maxHeight:120,overflow:"auto"}}>{candidates.map(r=><div key={r.candidate}>{r.candidateType} · {r.candidate} · {Math.round(r.score*100)}% · {r.evidence}</div>)}</div>}
    {error&&<p role="alert">{error}</p>}
  </section>
}
