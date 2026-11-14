//! Opt-in read-only WebDAV view of a unified encrypted logical backup.
//! Files stream verified age chunks; no decrypted file staging.
use super::{AppState,audit_actor};
use axum::{body::Body,extract::{Path,State},
    http::{header,HeaderMap,HeaderValue,Method,StatusCode},
    response::{IntoResponse,Response}};
use base64::{engine::general_purpose::STANDARD,Engine};
use bytes::Bytes;
use futures_util::stream;
use multi_backup_core::chunk_store::{Entry,LogicalBackup,Store};
use percent_encoding::{utf8_percent_encode,NON_ALPHANUMERIC};
use std::{collections::BTreeMap,env,fs,path::PathBuf,sync::Arc};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize,Zeroizing};
type Res<T>=Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Clone)]
pub struct DavConfig{
    user:String,
    token:Arc<Zeroizing<String>>,
}
impl DavConfig {
    pub fn from_env(origin:&str)->Res<Option<Self>>{
        let path=match env::var_os("MULTI_BACKUP_DAV_PASSWORD_FILE"){
            None=>return Ok(None),Some(v)=>PathBuf::from(v)
        };
        if !origin.starts_with("https://"){return Err("HTTPS required for live WebDAV mounts".into());}
        let user=env::var("MULTI_BACKUP_DAV_USER")?;
        if user.is_empty()||user.len()>100||
            !user.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-'||b==b'_'){
            return Err("Unsafe DAV username".into());
        }
        let meta=fs::symlink_metadata(&path)?;
        if !meta.file_type().is_file()||meta.len()>1024{
            return Err("DAV token file is unsafe".into());
        }
        let password=Zeroizing::new(fs::read_to_string(path)?.trim_end_matches(['\r','\n']).to_owned());
        if !(32..=512).contains(&password.len()){
            return Err("Mount token must have 32-512 characters".into());
        }
        Ok(Some(Self{user,token:Arc::new(password)}))
    }
    fn authorized(&self,h:&HeaderMap)->bool{
        let Some(auth)=h.get(header::AUTHORIZATION).and_then(|v|v.to_str().ok())
            .and_then(|v|v.strip_prefix("Basic ")) else{return false;};
        if auth.len()>2048{return false;}
        let Ok(data)=STANDARD.decode(auth) else{return false;};
        let mut plain=Zeroizing::new(data);
        let credentials=plain.splitn(2,|x|*x==b':').collect::<Vec<_>>();
        let allowed=credentials.len()==2 && credentials[0]==self.user.as_bytes() &&
            credentials[1].ct_eq(self.token.as_bytes()).unwrap_u8()==1;
        plain.zeroize();
        allowed
    }
}
fn deny(status:StatusCode)->Response{
    let mut res=status.into_response();
    res.headers_mut().insert(header::CACHE_CONTROL,HeaderValue::from_static("no-store"));
    if status==StatusCode::UNAUTHORIZED {
        res.headers_mut().insert(header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"Multi Backup Read-Only\""));
    }
    res
}
fn escaped(value:&str)->String{
    value.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;")
        .replace('"',"&quot;").replace('\'',"&apos;")
}
fn href(id:&str,virtual_path:&str,is_dir:bool)->String{
    let encoded=virtual_path.split('/').filter(|s|!s.is_empty())
        .map(|x|utf8_percent_encode(x,NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>().join("/");
    let mut output=format!("/dav/{id}");
    if !encoded.is_empty(){output.push('/');output.push_str(&encoded);}
    if is_dir{output.push('/');}
    output
}
fn item_xml(backup:&str,path:&str,entry:Option<&Entry>,directory:bool)->String{
    let name=path.split('/').last().filter(|s|!s.is_empty()).unwrap_or("Multi Backup");
    let kind=if directory{"<d:collection/>"}else{""};
    let size=entry.map(|x|x.bytes).unwrap_or(0);
    format!("<d:response><d:href>{}</d:href><d:propstat><d:prop><d:displayname>{}</d:displayname><d:resourcetype>{kind}</d:resourcetype><d:getcontentlength>{size}</d:getcontentlength></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response>",
        escaped(&href(backup,path,directory)),escaped(name))
}
fn listing(backup:&LogicalBackup,prefix:&str,depth:u8)->Option<String>{
    if depth>1{return None;}
    let file=backup.entries.iter().find(|e|e.path==prefix);
    let directory=prefix.is_empty()||
        backup.entries.iter().any(|e|e.path.starts_with(&format!("{prefix}/")));
    if !directory&&file.is_none(){return None;}
    let mut result=vec![item_xml(&backup.id,prefix,file,directory)];
    if directory&&depth==1{
        let mut children:BTreeMap<String,Option<&Entry>>=BTreeMap::new();
        let prefix_with_slash=if prefix.is_empty(){String::new()}else{format!("{prefix}/")};
        for entry in &backup.entries{
            if let Some(rel)=entry.path.strip_prefix(&prefix_with_slash){
                let segment=rel.split('/').next().unwrap_or("");
                if segment.is_empty(){continue;}
                let path=if prefix.is_empty(){segment.to_owned()}
                    else{format!("{prefix}/{segment}")};
                if rel.contains('/') {children.entry(path).or_insert(None);}
                else {children.insert(path,Some(entry));}
            }
        }
        for (path,entry) in children{
            result.push(item_xml(&backup.id,&path,entry,entry.is_none()));
        }
    }
    Some(format!("<?xml version=\"1.0\" encoding=\"utf-8\"?><d:multistatus xmlns:d=\"DAV:\">{}</d:multistatus>",result.join("")))
}
pub async fn root(State(state):State<AppState>,Path(id):Path<String>,
    method:Method,headers:HeaderMap)->Response{
    serve(state,id,String::new(),method,headers).await
}
pub async fn file(State(state):State<AppState>,
    Path((id,path)):Path<(String,String)>,method:Method,headers:HeaderMap)->Response{
    serve(state,id,path,method,headers).await
}
async fn serve(state:AppState,id:String,path:String,method:Method,headers:HeaderMap)->Response{
    if method==Method::OPTIONS{
        let mut res=StatusCode::NO_CONTENT.into_response();
        res.headers_mut().insert("dav",HeaderValue::from_static("1"));
        res.headers_mut().insert(header::ALLOW,
            HeaderValue::from_static("OPTIONS, PROPFIND, GET, HEAD"));
        return res;
    }
    let Some(ref config)=state.0.dav else{return deny(StatusCode::NOT_FOUND);};
    if !config.authorized(&headers){return deny(StatusCode::UNAUTHORIZED);}
    if id.len()!=64||!id.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase())||
        path.split('/').any(|x|x=="."||x==".."||x.contains('\\')||x.contains('\0')){
        return deny(StatusCode::BAD_REQUEST);
    }
    let Some(ref identity)=state.0.identity else{return deny(StatusCode::SERVICE_UNAVAILABLE);};
    let store=match Store::open(&state.0.root,identity){
        Ok(v)=>v,Err(_)=>return deny(StatusCode::SERVICE_UNAVAILABLE)
    };
    let backup=match store.load(identity,&id){
        Ok(v)=>v,Err(_)=>return deny(StatusCode::NOT_FOUND)
    };
    if method.as_str()=="PROPFIND" {
        let depth=match headers.get("depth").and_then(|v|v.to_str().ok()).unwrap_or("1"){
            "0"=>0,"1"=>1,_=>return deny(StatusCode::FORBIDDEN)
        };
        let Some(document)=listing(&backup,&path,depth) else{
            return deny(StatusCode::NOT_FOUND);
        };
        let mut res=(StatusCode::MULTI_STATUS,
            [(header::CONTENT_TYPE,"application/xml; charset=utf-8")],document).into_response();
        res.headers_mut().insert("dav",HeaderValue::from_static("1"));
        res.headers_mut().insert(header::CACHE_CONTROL,HeaderValue::from_static("no-store"));
        audit_actor(&state.0,"webdav_read_only_list",&config.user);
        return res;
    }
    if method!=Method::GET&&method!=Method::HEAD{
        return deny(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some(found)=backup.entries.into_iter().find(|e|e.path==path) else{
        return deny(StatusCode::NOT_FOUND);
    };
    let size=found.bytes;
    let body=if method==Method::HEAD{Body::empty()}else{
        let chunks=found.chunks;
        let state=(Arc::new(store),identity.clone(),chunks,0usize);
        let stream=stream::try_unfold(state,|mut value|async move{
            if value.3>=value.2.len(){return Ok::<_,std::io::Error>(None);}
            let chunk=value.2[value.3].clone();
            value.3+=1;
            let store=value.0.clone();let key=value.1.clone();
            let data=tokio::task::spawn_blocking(move||{
                store.read_chunk_ref(&key,&chunk)
                    .map_err(|e|std::io::Error::new(std::io::ErrorKind::InvalidData,e.to_string()))
            }).await.map_err(std::io::Error::other)??;
            Ok(Some((Bytes::from(data),value)))
        });
        Body::from_stream(stream)
    };
    let mut res=body.into_response();
    if let Ok(value)=size.to_string().parse(){
        res.headers_mut().insert(header::CONTENT_LENGTH,value);
    }
    res.headers_mut().insert(header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"));
    res.headers_mut().insert(header::CACHE_CONTROL,HeaderValue::from_static("no-store"));
    audit_actor(&state.0,"webdav_read_only_file",&config.user);
    res
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn unified_virtual_folders_in_dav_xml(){
        let b=LogicalBackup{version:1,id:"a".repeat(64),label:"unified".into(),
            entries:vec![
                Entry{path:"github/repo/README.md".into(),bytes:1,chunks:vec![]},
                Entry{path:"ci/run.json".into(),bytes:2,chunks:vec![]},
                Entry{path:"dropbox/item.txt".into(),bytes:3,chunks:vec![]}
            ]};
        let root=listing(&b,"",1).unwrap();
        assert!(root.contains("github/")&&root.contains("ci/")&&root.contains("dropbox/"));
        assert!(listing(&b,"github/repo",1).unwrap().contains("README.md"));
        assert!(listing(&b,"x",1).is_none());
    }
    #[test]
    fn xml_escaping_avoids_injection(){assert_eq!(escaped("x&<"),"x&amp;&lt;");}
}
