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

const mbMonitorOut=document.getElementById("monitor-settings-result");
const mbTokenOut=document.getElementById("monitor-token-result");
async function mbMonitorFetch(path,method,body){
  const opts={credentials:"same-origin",cache:"no-store"};
  if(method!=="GET"){opts.method=method;opts.headers={"Content-Type":"application/json","X-CSRF-Token":csrf};opts.body=JSON.stringify(body);}
  const result=await fetch(path,opts);
  if(!result.ok)throw new Error("HTTP "+result.status);
  return result.json();
}
document.getElementById("monitor-refresh").addEventListener("click",async()=>{
  try{const obj=await mbMonitorFetch("/api/monitor/settings","GET");mbMonitorOut.textContent=JSON.stringify(obj,null,2);}
  catch(e){mbMonitorOut.textContent="Monitoraggio non disponibile: "+e.message;}
});
document.getElementById("monitor-token-issue").addEventListener("click",async()=>{
  try{
    const days=Number(document.getElementById("monitor-token-days").value);
    const obj=await mbMonitorFetch("/api/monitor/tokens","POST",{ttl_days:days});
    mbTokenOut.textContent="Token REST (visibile una sola volta): "+obj.token_once+
      "\nID: "+obj.id+"\nScadenza: "+new Date(obj.expires_unix*1000).toISOString();
  }catch(e){mbTokenOut.textContent="Creazione non riuscita: "+e.message;}
});
document.getElementById("monitor-token-list").addEventListener("click",async()=>{
  try{const obj=await mbMonitorFetch("/api/monitor/tokens","GET");mbTokenOut.textContent=JSON.stringify(obj,null,2);}
  catch(e){mbTokenOut.textContent=e.message;}
});
document.getElementById("monitor-token-revoke").addEventListener("click",async()=>{
  try{
    const id=document.getElementById("monitor-token-revoke-id").value.trim();
    const obj=await mbMonitorFetch("/api/monitor/tokens/revoke","POST",{id});
    mbTokenOut.textContent=obj.revoked?"Token revocato":"Token non trovato";
  }catch(e){mbTokenOut.textContent=e.message;}
});
