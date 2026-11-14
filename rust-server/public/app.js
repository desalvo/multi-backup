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
