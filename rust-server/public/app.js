"use strict";
const csrf=document.querySelector('meta[name=csrf-token]').content;
const backups=document.getElementById("backups"),notice=document.getElementById("alert");
const search=document.getElementById("search"),view=document.getElementById("view");
const selectedCount=document.getElementById("selected-count"),exportButton=document.getElementById("export-selected");
let records=[],activeBackup=null,selected=new Set(),grid=false;
function el(tag,text,cls){
  const n=document.createElement(tag);if(text!==undefined)n.textContent=text;
  if(cls)n.className=cls;return n;
}
function match(path){return path.toLocaleLowerCase().includes(search.value.trim().toLocaleLowerCase());}
function folderTree(files){
  const root={files:[],folders:new Map()};
  for(const file of files){
    if(!match(file.path))continue;
    const parts=file.path.split("/");let node=root;
    for(const name of parts.slice(0,-1)){
      if(!node.folders.has(name))node.folders.set(name,{files:[],folders:new Map()});
      node=node.folders.get(name);
    }
    node.files.push(file);
  }
  return root;
}
function choose(path,checked){
  if(checked)selected.add(path);else selected.delete(path);
  selectedCount.textContent=selected.size+" elementi selezionati";
  exportButton.disabled=selected.size===0;
}
function treeView(node,parent,prefix){
  for(const [name,child] of [...node.folders.entries()].sort((a,b)=>a[0].localeCompare(b[0]))){
    const path=prefix?prefix+"/"+name:name;
    const folder=el("details",undefined,"virtual-folder");folder.open=prefix.split("/").length<2;
    const summary=el("summary");
    const checkbox=document.createElement("input");checkbox.type="checkbox";checkbox.setAttribute("aria-label","Seleziona cartella "+path);
    checkbox.checked=selected.has(path);
    checkbox.addEventListener("click",e=>e.stopPropagation());
    checkbox.addEventListener("change",()=>choose(path,checkbox.checked));
    summary.append(checkbox,el("span","📁 "+name));folder.append(summary);
    treeView(child,folder,path);parent.append(folder);
  }
  for(const file of node.files.sort((a,b)=>a.path.localeCompare(b.path))){
    const row=el("div",undefined,"virtual-file");
    const box=document.createElement("input");box.type="checkbox";box.checked=selected.has(file.path);
    box.setAttribute("aria-label","Seleziona "+file.path);
    box.addEventListener("change",()=>choose(file.path,box.checked));
    const name=el("span","📄 "+file.path.split("/").at(-1));
    name.title=file.path;
    row.append(box,name,el("small",file.size+" B","muted"));
    const button=el("button","Esporta");button.type="button";
    button.addEventListener("click",()=>exportPaths([file.path]));
    row.append(button);parent.append(row);
  }
}
function render(){
  backups.replaceChildren();
  for(const backup of records){
    const panel=el("section",undefined,"logical-backup");
    const title=el("h3","📦 "+backup.label+" · "+backup.files.length+" file");
    const meta=el("div","Identificativo: "+backup.id,"small muted");
    const controls=el("div",undefined,"controls");
    const open=el("button",activeBackup===backup.id?"Backup selezionato":"Seleziona backup");
    open.type="button";
    open.addEventListener("click",()=>{activeBackup=backup.id;selected.clear();choose("",false);render();});
    const full=el("button","Esporta tutto");full.type="button";
    full.addEventListener("click",()=>exportPaths([],backup.id));
    controls.append(open,full);panel.append(title,meta,controls);
    if(activeBackup===backup.id){
      const area=el("div",undefined,grid?"virtual-grid":"virtual-list");
      treeView(folderTree(backup.files),area,"");
      panel.append(area);
    }
    backups.append(panel);
  }
}
async function load(){
  notice.textContent="Verifica dei manifest logici...";
  const response=await fetch("/api/backups",{credentials:"same-origin"});
  if(!response.ok){notice.textContent="Impossibile leggere lo storage o il vault age";return;}
  const content=await response.json();
  records=content.backups;
  if(!records.some(r=>r.id===activeBackup))activeBackup=records[0]?.id||null;
  selected.clear();selectedCount.textContent="0 elementi selezionati";exportButton.disabled=true;
  notice.textContent=records.length+" backup logici verificati";
  render();
}
async function exportPaths(paths,id=activeBackup){
  if(!id){return;}
  const state=document.getElementById("result");
  state.textContent="Ricostruzione e verifica in corso...";
  const approval=document.getElementById("recovery-authorization");
  const recoveryCode=approval.value.trim();
  approval.value="";
  const headers={"Content-Type":"application/json","X-CSRF-Token":csrf};
  if(recoveryCode)headers["X-Multi-Backup-Recovery-Authorization"]=recoveryCode;
  let response;
  try {
    response=await fetch("/api/export",{method:"POST",credentials:"same-origin",
      headers,body:JSON.stringify({backup_id:id,selections:paths})});
  } finally {
    headers["X-Multi-Backup-Recovery-Authorization"]="";
  }
  const body=await response.json();
  state.textContent=response.ok?("Ripristinati "+body.files+" file in "+body.server_directory):
    ("Errore: "+body.error);
}
document.getElementById("refresh").addEventListener("click",()=>load().catch(e=>notice.textContent=e.message));
exportButton.addEventListener("click",()=>exportPaths([...selected]));
view.addEventListener("click",()=>{grid=!grid;view.setAttribute("aria-pressed",String(grid));
  view.textContent=grid?"Vista a lista":"Vista a icone";render();});
search.addEventListener("input",render);
document.getElementById("logout").addEventListener("click",async()=>{
  await fetch("/logout",{method:"POST",credentials:"same-origin",headers:{"X-CSRF-Token":csrf}});
  window.location.assign("/login");
});
load().catch(e=>notice.textContent=e.message);

const smtpButton=document.getElementById("smtp-test");
smtpButton.addEventListener("click",async()=>{
  const report=document.getElementById("smtp-result");
  smtpButton.disabled=true;report.textContent="Invio SMTP(S) in corso...";
  try {
    const response=await fetch("/api/smtp/test",{method:"POST",credentials:"same-origin",
      headers:{"X-CSRF-Token":csrf}});
    report.textContent=response.ok?"Email di prova inviata":"Errore SMTP: verificare host, TLS e secret";
  } catch(error) {report.textContent="SMTP non disponibile";}
  finally {smtpButton.disabled=false;}
});

let auditOffset=0;
async function showAudit(){
  const output=document.getElementById("audit-events");
  const q=document.getElementById("audit-search").value.trim();
  const url="/api/audit?offset="+auditOffset+"&limit=10&query="+encodeURIComponent(q);
  const result=await fetch(url,{credentials:"same-origin"});
  if(result.status===403){output.textContent="Audit disponibile solo all'amministratore.";return;}
  if(!result.ok){output.textContent="Audit non disponibile.";return;}
  const data=await result.json();
  output.replaceChildren();
  output.append(el("p","Eventi: "+data.total+" · pagina "+(auditOffset/10+1),"muted"));
  for(const item of data.events){
    const row=el("div",undefined,"virtual-file");
    row.append(el("span",String(item.time||"")),
        el("strong",String(item.event||"")),
        el("span",String(item.user||"")));
    output.append(row);
  }
  document.getElementById("audit-older").disabled=(auditOffset+10>=data.total);
}
document.getElementById("audit-refresh").addEventListener("click",()=>{
  auditOffset=0;showAudit().catch(()=>{});
});
document.getElementById("audit-older").addEventListener("click",()=>{
  auditOffset+=10;showAudit().catch(()=>{});
});


// R039: same cluster view on every pod. Read-only admin information;
// DOM textContent avoids interpreting provider-supplied names as HTML.
const clusterRefresh=document.getElementById("cluster-refresh");
function clusterLine(parent,headline,detail,status){
  const card=el("div",undefined,"cluster-record");
  const heading=el("strong",headline);
  const sub=el("small",detail||"","muted");
  if(status){card.dataset.status=status;}
  card.append(heading,sub);parent.append(card);
}
async function loadCluster(){
  const summary=document.getElementById("cluster-summary");
  const output={
    replicas:document.getElementById("cluster-replicas"),
    storage:document.getElementById("cluster-storage"),
    jobs:document.getElementById("cluster-jobs"),
    agents:document.getElementById("cluster-agents")
  };
  clusterRefresh.disabled=true;
  try {
    const [clusterResponse,agentsResponse]=await Promise.all([
      fetch("/api/cluster/status",{credentials:"same-origin",cache:"no-store"}),
      fetch("/api/agents/admin/list",{credentials:"same-origin",cache:"no-store"})
    ]);
    if(clusterResponse.status===401||clusterResponse.status===403){
      summary.textContent="Il pannello cluster è riservato all'amministratore.";
      for(const container of Object.values(output))container.replaceChildren();
      return;
    }
    if(!clusterResponse.ok){
      summary.textContent="Cluster non configurato o non disponibile su questa replica.";
      for(const container of Object.values(output))container.replaceChildren();
      return;
    }
    const [cluster,registered]=await Promise.all([
      clusterResponse.json(),
      agentsResponse.ok?agentsResponse.json():Promise.resolve({agents:[]})
    ]);
    for(const container of Object.values(output))container.replaceChildren();
    const nodes=Array.isArray(cluster.replicas)?cluster.replicas:[];
    const stores=Array.isArray(cluster.storage)?cluster.storage:[];
    const jobs=Array.isArray(cluster.jobs)?cluster.jobs:[];
    const agents=Array.isArray(registered.agents)?registered.agents:[];
    const active=nodes.filter(node=>node.healthy).length;
    summary.textContent=active+"/"+nodes.length+" repliche attive · "+jobs.length+
      " job recenti · "+agents.filter(a=>a.approved&&!a.revoked).length+
      " agenti autorizzati";
    for(const node of nodes){
      clusterLine(output.replicas,(node.healthy?"● ":"○ ")+String(node.id),
        "Capacità "+String(node.max_jobs)+" job · storage: "+
        (Array.isArray(node.storage_ids)?node.storage_ids.join(", "):"nessuno"),
        node.healthy?"online":"offline");
    }
    for(const storage of stores){
      const isLocal=storage.visibility==="node_local";
      clusterLine(output.storage,String(storage.id)+" · "+String(storage.backend),
        isLocal?"Locale · owner "+String(storage.owner_replica):
          "Condiviso · accesso verificato tramite registro",
        isLocal?"local":"shared");
    }
    for(const job of jobs.slice(0,40)){
      clusterLine(output.jobs,String(job.id)+" · "+String(job.status),
        String(job.plugin)+" · "+String(job.storage)+" · replica "+
        String(job.replica||"in coda")+" · "+String(job.transport),
        String(job.status));
    }
    for(const agent of agents){
      const state=agent.revoked?"revocato":agent.approved?
        (agent.online?"online":"offline"):"in attesa";
      clusterLine(output.agents,String(agent.id)+" · "+state,
        "Utente "+String(agent.user)+" · seq "+String(agent.last_sequence),
        state);
    }
    if(!nodes.length)clusterLine(output.replicas,"Nessuna replica","Verificare PostgreSQL e heartbeat");
    if(!stores.length)clusterLine(output.storage,"Nessuno storage","Registrare prima i backend condivisi o locali");
    if(!jobs.length)clusterLine(output.jobs,"Nessun job","La coda non contiene lavori recenti");
    if(!agents.length)clusterLine(output.agents,"Nessun agente","Registrazione e approvazione richieste");
  }catch(error){
    summary.textContent="Impossibile aggiornare lo stato del cluster.";
  }finally{clusterRefresh.disabled=false;}
}
clusterRefresh.addEventListener("click",loadCluster);
loadCluster();
document.addEventListener("visibilitychange",()=>{
  if(!document.hidden)loadCluster();
});

// Browser token management is independent of the monitoring listener.
// Raw monitoring tokens are displayed once, never saved in localStorage.
async function loadMonitoringTokens(){
  const list=document.getElementById("monitor-token-list");
  const r=await fetch("/api/monitoring/tokens",{credentials:"same-origin",cache:"no-store"});
  list.replaceChildren();
  if(!r.ok){list.append(el("p","Registro token non disponibile.","muted"));return;}
  const data=await r.json();
  for(const token of (data.tokens||[])){
    const box=el("div",undefined,"cluster-record");
    box.append(el("strong",String(token.name)+" · "+(token.revoked?"revocato":"attivo")),
      el("small","Scadenza "+String(token.expires_at)+" · "+String(token.id),"muted"));
    if(!token.revoked){
      const b=el("button","Revoca");
      b.addEventListener("click",async()=>{
        const code=document.getElementById("recovery-authorization");
        const headers={"Content-Type":"application/json","X-CSRF-Token":csrf,
          "X-Multi-Backup-Recovery-Authorization":code.value.trim()};
        code.value="";
        const resp=await fetch("/api/monitoring/tokens/revoke",{
          method:"POST",credentials:"same-origin",headers,
          body:JSON.stringify({id:token.id})});
        headers["X-Multi-Backup-Recovery-Authorization"]="";
        document.getElementById("monitor-token-result").textContent=
          resp.ok?"Token revocato":"Revoca rifiutata (serve approvazione offline).";
        loadMonitoringTokens();
      });
      box.append(b);
    }
    list.append(box);
  }
}
document.getElementById("monitoring-tokens-refresh").addEventListener("click",loadMonitoringTokens);
document.getElementById("monitor-token-create").addEventListener("click",async()=>{
  const display=document.getElementById("monitor-token-result");
  display.replaceChildren();
  const approval=document.getElementById("recovery-authorization");
  const headers={"Content-Type":"application/json","X-CSRF-Token":csrf,
    "X-Multi-Backup-Recovery-Authorization":approval.value.trim()};
  approval.value="";
  const response=await fetch("/api/monitoring/tokens",{
    method:"POST",credentials:"same-origin",headers,
    body:JSON.stringify({
      name:document.getElementById("monitor-token-name").value.trim(),
      lifetime_days:Number(document.getElementById("monitor-token-days").value)
    })
  });
  headers["X-Multi-Backup-Recovery-Authorization"]="";
  if(!response.ok){display.textContent="Creazione token rifiutata.";return;}
  const data=await response.json();
  display.append(el("strong","Token creato: copialo ORA, non sarà più recuperabile."));
  const input=el("input");input.readOnly=true;input.type="text";input.value=String(data.token||"");
  display.append(input);
  const copy=el("button","Copia token");
  copy.addEventListener("click",()=>navigator.clipboard.writeText(input.value));
  display.append(copy);
  loadMonitoringTokens();
});
loadMonitoringTokens().catch(()=>{});

// PostgreSQL-authoritative backup metadata, identical on each bare-metal
// server or Kubernetes replica. Never offer a local restore for a remote
// node-local volume; that needs an authenticated owner routing operation.
async function loadClusterBackups(){
  const summary=document.getElementById("cluster-backup-summary");
  const body=document.getElementById("cluster-backup-inventory");
  try{
    const response=await fetch("/api/cluster/backups",{
      credentials:"same-origin",cache:"no-store"
    });
    if(!response.ok){
      summary.textContent="Catalogo cluster non configurato o non disponibile.";
      body.replaceChildren();return;
    }
    const catalog=await response.json();
    const items=Array.isArray(catalog.backups)?catalog.backups:[];
    summary.textContent=items.length+" backup nel catalogo unificato PostgreSQL";
    body.replaceChildren();
    for(const item of items.slice(0,100)){
      const node=el("div",undefined,"cluster-record");
      node.append(el("strong",String(item.label||item.id)),
        el("small",String(item.owner_replica)+" · "+
          String(item.storage_id)+" · "+
          (item.owner_online?"nodo online":"nodo offline")+" · "+
          (Array.isArray(item.files)?item.files.length:0)+" file","muted"),
        el("small","Solo inventario verificato: restore remoto non ancora abilitato","muted"));
      body.append(node);
    }
  }catch(_){
    summary.textContent="Catalogo cluster temporaneamente non disponibile.";
  }
}
document.getElementById("cluster-refresh").addEventListener("click",loadClusterBackups);
loadClusterBackups();

// Primary/replica status and scheduled encrypted internal DB backups.
// Query PostgreSQL via the shared authenticated server API. This works from
// any physical server or Kubernetes replica, without node-specific URLs.
async function internalDbRefresh(){
  const health=document.getElementById("internal-db-health");
  const standby=document.getElementById("internal-db-standbys");
  const history=document.getElementById("internal-db-history");
  try{
    const response=await fetch("/api/internal-db/status",{cache:"no-store",credentials:"same-origin"});
    if(!response.ok){
      health.textContent="Database cluster non disponibile o permessi insufficienti.";
      standby.replaceChildren();history.replaceChildren();return;
    }
    const db=await response.json();
    const state=db.database||{};
    health.textContent="PostgreSQL: "+String(state.role||"sconosciuto")+
      " · sync commit "+String(state.synchronous_commit||"non disponibile")+
      " · job backup "+String(db.lease_state||"n/d")+
      " · prossimo backup "+String(db.next_due||"non disponibile");
    standby.replaceChildren();
    const nodes=Array.isArray(state.standbys)?state.standbys:[];
    for(const node of nodes){
      const row=el("div",undefined,"cluster-record");
      row.append(el("strong",String(node.name||"replica")+" · "+
        String(node.sync_state||"n/d")),
        el("small",String(node.state||"n/d")+" · "+
          String(node.lag_bytes||0)+" byte di ritardo WAL","muted"));
      standby.append(row);
    }
    if(!nodes.length)standby.append(el("small",
       "Nessuna replica rilevata: controllare Patroni, permessi pg_monitor e HAProxy","muted"));
    document.getElementById("internal-db-enabled").checked=Boolean(db.enabled);
    document.getElementById("internal-db-interval").value=String(db.interval_minutes||1440);
    history.replaceChildren();
    for(const item of (Array.isArray(db.history)?db.history:[]).slice(0,40)){
      const row=el("div",undefined,"cluster-record");
      row.append(el("strong",String(item.status)+" · "+String(item.started_at||"")),
        el("small","Backup "+String(item.backup_id||"da verificare")+
          " · "+String(item.replica||"")+" · "+String(item.target),"muted"));
      history.append(row);
    }
  }catch(_){
    health.textContent="Impossibile contattare il catalogo HA PostgreSQL.";
  }
}
async function internalDbWrite(operation){
  const result=document.getElementById("internal-db-feedback");
  const save=document.getElementById("internal-db-save");
  const run=document.getElementById("internal-db-run");
  save.disabled=true;run.disabled=true;
  const headers={"X-CSRF-Token":csrf,"Content-Type":"application/json"};
  // The offline approval value is not persisted, displayed or logged.
  const approval=document.getElementById("recovery-authorization");
  headers["X-Multi-Backup-Recovery-Authorization"]=approval.value.trim();
  approval.value="";
  try{
    const req=operation==="policy"?{
      enabled:document.getElementById("internal-db-enabled").checked,
      interval_minutes:Number(document.getElementById("internal-db-interval").value),
      target:"s3_worm"
    }:{};
    const resp=await fetch("/api/internal-db/"+operation,{
      method:"POST",credentials:"same-origin",headers,body:JSON.stringify(req)
    });
    const obj=await resp.json();
    result.textContent=resp.ok?
      (operation==="backup"?"Backup database accodato: "+String(obj.job_id):
       "Piano salvato e condiviso fra le repliche"):
      "Operazione rifiutata: "+String(obj.error||resp.status);
    if(resp.ok)await internalDbRefresh();
  }catch(_){result.textContent="Errore durante la modifica del piano database.";}
  finally{headers["X-Multi-Backup-Recovery-Authorization"]="";save.disabled=false;run.disabled=false;}
}
document.getElementById("internal-db-refresh").addEventListener("click",internalDbRefresh);
document.getElementById("internal-db-save").addEventListener("click",()=>internalDbWrite("policy"));
document.getElementById("internal-db-run").addEventListener("click",()=>internalDbWrite("backup"));
internalDbRefresh();

// Vault backup storage is separate from the password-protected age identity vault.
const vaultStatus=document.getElementById("vault-state");
const vaultRows=document.getElementById("vault-snapshots");
async function loadImmutableVault(){
  const status=await fetch("/api/vault-store/status",{credentials:"same-origin",cache:"no-store"});
  if(!status.ok){
    vaultStatus.textContent=status.status===403?"Solo amministratore":"Vault append-only non configurato";
    vaultRows.replaceChildren();return;
  }
  const info=await status.json();
  vaultStatus.textContent="Deposito configurato · external WORM verificato: "+
    (info.external_worm_verified?"sì":"NO — anteprima, protezione storage indipendente necessaria");
  const response=await fetch("/api/vault-store/list",{credentials:"same-origin",cache:"no-store"});
  if(!response.ok){vaultRows.textContent="Impossibile leggere manifest vault";return;}
  const values=await response.json();
  vaultRows.replaceChildren();
  for(const record of values.snapshots||[]){
    const row=el("div",undefined,"virtual-file");
    const select=el("button","Seleziona");
    select.addEventListener("click",()=>{document.getElementById("vault-snapshot").value=record.id;});
    row.append(el("span",record.id),el("small",String(record.encrypted_bytes)+" B","muted"),select);
    vaultRows.append(row);
  }
}
async function vaultMutation(operation){
  vaultStatus.textContent="Operazione vault in corso...";
  const headers={"X-CSRF-Token":csrf,"Content-Type":"application/json"};
  const request={method:"POST",credentials:"same-origin",headers};
  if(operation==="restore"){
    const id=document.getElementById("vault-snapshot").value.trim();
    if(!/^[0-9a-f]{32}$/.test(id)){vaultStatus.textContent="ID snapshot vault non valido";return;}
    const input=document.getElementById("recovery-authorization");
    const approval=input.value.trim();input.value="";
    if(approval)headers["X-Multi-Backup-Recovery-Authorization"]=approval;
    request.body=JSON.stringify({id});
  }
  try{
    const response=await fetch("/api/vault-store/"+operation,request);
    const data=await response.json();
    vaultStatus.textContent=response.ok?(
      operation==="backup"?"Nuovo backup vault: "+data.id+" · WORM esterno non verificato":
        "Restore in cartella NUOVA: "+data.new_restore_directory):
      "Errore: "+String(data.error||"vault_unavailable");
    if(response.ok)await loadImmutableVault();
  }catch(error){vaultStatus.textContent="Connessione vault non disponibile";}
  finally{headers["X-Multi-Backup-Recovery-Authorization"]="";}
}
document.getElementById("vault-refresh").addEventListener("click",()=>loadImmutableVault().catch(()=>{}));
document.getElementById("vault-backup").addEventListener("click",()=>vaultMutation("backup"));
document.getElementById("vault-restore").addEventListener("click",()=>vaultMutation("restore"));
loadImmutableVault().catch(()=>{vaultStatus.textContent="Vault non disponibile";});


// r218: server-side cloud file browser; fixed pre-authorized rclone profiles only.
// All provider names/paths are rendered with textContent, never as HTML.
const cloudProfile=document.getElementById("cloud-profile");
const cloudPath=document.getElementById("cloud-path");
const cloudFeedback=document.getElementById("cloud-feedback");
const cloudItems=document.getElementById("cloud-items");
let cloudProfiles=[];
function cloudSelected(){return cloudProfiles.find(p=>p.id===cloudProfile.value);}
function cloudRemotePath(name){
  const base=cloudPath.value.trim().replace(/\/+$/,"");
  return base?base+"/"+name:name;
}
function cloudApprovalHeaders(){
  const approval=document.getElementById("recovery-authorization");
  const headers={"X-CSRF-Token":csrf,
    "X-Multi-Backup-Recovery-Authorization":approval.value.trim()};
  approval.value="";
  return headers;
}
async function cloudLoadProfiles(){
  const r=await fetch("/api/cloud/profiles",{credentials:"same-origin",cache:"no-store"});
  cloudItems.replaceChildren();cloudProfile.replaceChildren();
  if(!r.ok){cloudFeedback.textContent="Cloud browser non configurato o permesso negato.";return;}
  const data=await r.json();
  cloudProfiles=Array.isArray(data.profiles)?data.profiles:[];
  for(const p of cloudProfiles){
    const option=el("option",p.id+" ("+p.provider+")");
    option.value=p.id;cloudProfile.append(option);
  }
  cloudFeedback.textContent=cloudProfiles.length+" cloud autorizzati";
  if(cloudProfiles.length)await cloudList();
}
async function cloudList(){
  const p=cloudSelected(),cap=document.getElementById("cloud-capabilities");
  if(!p){cloudItems.replaceChildren();return;}
  cap.textContent=(p.upload?"Upload permesso":"Sola lettura")+
    " · "+(p.delete?"Cancellazione abilitata":"Cancellazione disabilitata")+
    (p.mount_read_only?" · mount RO":" · mount RW consentito dalla policy");
  const params=new URLSearchParams({profile:p.id,path:cloudPath.value.trim()});
  const response=await fetch("/api/cloud/list?"+params,{credentials:"same-origin",cache:"no-store"});
  cloudItems.replaceChildren();
  if(!response.ok){cloudFeedback.textContent="Directory non disponibile o percorso non valido.";return;}
  const data=await response.json();
  const items=Array.isArray(data.items)?data.items:[];
  cloudFeedback.textContent=items.length+" elementi in /"+(data.path||"");
  for(const item of items){
    const line=el("div",undefined,"cloud-file");
    const name=el("button",(item.directory?"📁 ":"📄 ")+String(item.name));
    name.type="button";name.className="cloud-file-name";
    name.addEventListener("click",()=>{
      if(item.directory){cloudPath.value=cloudRemotePath(String(item.name));cloudList();}
      else {cloudDownload(cloudRemotePath(String(item.name)));}
    });
    const size=el("small",item.directory?"Cartella":String(item.size)+" B","muted");
    line.append(name,size);
    if(!item.directory){
      const dl=el("button","Scarica");dl.type="button";
      dl.addEventListener("click",()=>cloudDownload(cloudRemotePath(String(item.name))));
      line.append(dl);
      if(p.delete){
        const del=el("button","Elimina");del.type="button";
        del.addEventListener("click",()=>cloudDelete(cloudRemotePath(String(item.name))));
        line.append(del);
      }
    }
    cloudItems.append(line);
  }
  if(!items.length)cloudItems.append(el("p","Directory vuota o non sincronizzata.","muted"));
}
function cloudDownload(path){
  const p=cloudSelected();if(!p)return;
  const url="/api/cloud/download?"+new URLSearchParams({profile:p.id,path});
  window.location.assign(url);
}
async function cloudUpload(){
  const p=cloudSelected(),file=document.getElementById("cloud-upload-file").files[0];
  if(!p||!p.upload){cloudFeedback.textContent="Upload disabilitato dalla policy.";return;}
  if(!file||!file.size||file.size>8*1024*1024){cloudFeedback.textContent="Seleziona un file tra 1 B e 8 MiB.";return;}
  const path=cloudRemotePath(file.name);
  const headers=cloudApprovalHeaders();
  try{
    const res=await fetch("/api/cloud/upload?"+new URLSearchParams({profile:p.id,path}),{
      method:"POST",credentials:"same-origin",headers,body:file
    });
    const result=await res.json();
    cloudFeedback.textContent=res.ok?"File caricato in nuovo percorso (preview).":
      "Upload rifiutato: "+String(result.error||res.status);
    if(res.ok)await cloudList();
  }finally{headers["X-Multi-Backup-Recovery-Authorization"]="";}
}
async function cloudDelete(path){
  const p=cloudSelected();if(!p?.delete)return;
  if(path.endsWith(".age")||path.endsWith(".age.sha256")){
    cloudFeedback.textContent="Un backup cifrato si elimina solo con la retention.";return;
  }
  const required="DELETE:"+p.id+":"+path;
  const confirmation=window.prompt("Per cancellare definitivamente inserisci esattamente:\n"+required);
  if(confirmation!==required)return;
  const headers=cloudApprovalHeaders();
  headers["Content-Type"]="application/json";
  try{
    const res=await fetch("/api/cloud/delete",{method:"POST",
      credentials:"same-origin",headers,
      body:JSON.stringify({profile:p.id,path,confirm:confirmation})});
    cloudFeedback.textContent=res.ok?"File eliminato.":"Cancellazione rifiutata.";
    if(res.ok)await cloudList();
  }finally{headers["X-Multi-Backup-Recovery-Authorization"]="";}
}
document.getElementById("cloud-refresh").addEventListener("click",()=>cloudLoadProfiles().catch(()=>{}));
document.getElementById("cloud-list").addEventListener("click",()=>cloudList().catch(()=>{cloudFeedback.textContent="Errore di accesso.";}));
document.getElementById("cloud-back").addEventListener("click",()=>{
  const segments=cloudPath.value.trim().split("/").filter(Boolean);
  segments.pop();cloudPath.value=segments.join("/");cloudList();
});
cloudProfile.addEventListener("change",()=>{cloudPath.value="";cloudList();});
document.getElementById("cloud-upload").addEventListener("click",()=>cloudUpload().catch(()=>{cloudFeedback.textContent="Errore di upload.";}));
cloudLoadProfiles().catch(()=>{cloudFeedback.textContent="Cloud browser non disponibile.";});
