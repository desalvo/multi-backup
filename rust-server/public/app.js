"use strict";
const csrf=document.querySelector('meta[name=csrf-token]').content;
const backups=document.getElementById("backups"), alertNode=document.getElementById("alert");
async function load(){
  alertNode.textContent="Lettura manifest cifrati...";
  backups.replaceChildren();
  let response=await fetch("/api/backups",{credentials:"same-origin"});
  if(!response.ok){alertNode.textContent="Storage non accessibile o identità age non configurata";return;}
  const data=await response.json();alertNode.textContent=data.backups.length+" backup logici";
  for(const backup of data.backups){
    const box=document.createElement("div");box.className="folder";
    const heading=document.createElement("h3");heading.textContent="📦 "+backup.label+" · "+backup.files.length+" file";
    box.append(heading);
    const summary=document.createElement("p");summary.className="small muted";summary.textContent="ID: "+backup.id;box.append(summary);
    for(const file of backup.files){
      const line=document.createElement("div");line.className="file";
      const name=document.createElement("span");name.textContent="📄 "+file.path+" ("+file.size+" byte)";
      const button=document.createElement("button");button.type="button";button.textContent="Esporta";
      button.addEventListener("click",()=>exportFile(backup.id,file.path));
      line.append(name,button);box.append(line);
    }
    const all=document.createElement("button");all.type="button";all.textContent="Esporta backup completo";
    all.addEventListener("click",()=>exportFile(backup.id,null));box.append(all);
    backups.append(box);
  }
}
async function exportFile(backup_id,selection){
  const result=document.getElementById("result");result.textContent="Verifica integrità ed esportazione in corso...";
  const response=await fetch("/api/export",{method:"POST",credentials:"same-origin",
    headers:{"Content-Type":"application/json","X-CSRF-Token":csrf},
    body:JSON.stringify({backup_id,selection})});
  const resultJson=await response.json();
  result.textContent=response.ok ? ("Esportati "+resultJson.files+" file in "+resultJson.server_directory)
     : ("Esportazione non riuscita: "+resultJson.error);
}
document.getElementById("refresh").addEventListener("click",()=>load().catch(e=>alertNode.textContent=e.message));
document.getElementById("logout").addEventListener("click",async()=>{
  await fetch("/logout",{method:"POST",credentials:"same-origin",headers:{"X-CSRF-Token":csrf}});
  window.location.assign("/login");
});
load().catch(e=>alertNode.textContent=e.message);
