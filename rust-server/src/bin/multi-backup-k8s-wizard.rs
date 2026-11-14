//! Offline interactive installer for Kustomize overlay. Does not hold secrets.
use std::{fs, io::{self,Write},path::PathBuf};
fn ask(label:&str,default:&str)->io::Result<String>{
    print!("{label} [{default}]: ");io::stdout().flush()?;
    let mut input=String::new();io::stdin().read_line(&mut input)?;
    let v=input.trim();
    Ok(if v.is_empty(){default.into()}else{v.into()})
}
fn validate_token(s:&str)->bool{
    !s.is_empty() && s.len()<=63 && s.bytes().all(|b|
        b.is_ascii_lowercase()||b.is_ascii_digit()||b==b'-')
}
fn main()->Result<(),Box<dyn std::error::Error>>{
    println!("Multi Backup Kubernetes wizard (generates files; never applies or stores credentials)");
    let namespace=ask("Kubernetes namespace","multi-backup")?;
    let host=ask("HTTPS hostname","multi-backup.example.org")?;
    let ingress=ask("Ingress class (nginx/haproxy)","nginx")?;
    let storage=ask("PVC size","100Gi")?;
    let image=ask("Image tag","early-release")?;
    if !validate_token(&namespace) || !host.split('.').all(validate_token) ||
        !["nginx","haproxy"].contains(&ingress.as_str()) ||
        !storage.bytes().all(|x|x.is_ascii_digit()||x==b'G'||x==b'M'||x==b'T'||x==b'i') ||
        storage.is_empty() || !validate_token(&image) {
        return Err("Invalid input: restricted DNS, class, quantity or tag".into());
    }
    let out=PathBuf::from(ask("New output directory","./multi-backup-overlay")?);
    if out.exists(){return Err("Output directory must not exist (avoid overwrite)".into());}
    fs::create_dir(&out)?;
    let base=std::env::current_dir()?.join("deploy/kustomize/base");
    let patch=format!(r#"apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization
namespace: {namespace}
resources:
  - {base}
images:
  - name: desalvo/multi-backup
    newTag: {image}
patches:
  - target: {{kind: Ingress, name: multi-backup}}
    patch: |-
      - op: replace
        path: /spec/ingressClassName
        value: {ingress}
      - op: replace
        path: /metadata/annotations/kubernetes.io~1ingress.class
        value: {ingress}
      - op: replace
        path: /spec/rules/0/host
        value: {host}
      - op: replace
        path: /spec/tls/0/hosts/0
        value: {host}
  - target: {{kind: Deployment, name: multi-backup}}
    patch: |-
      - op: replace
        path: /spec/template/spec/containers/0/env/1/value
        value: https://{host}
  - target: {{kind: PersistentVolumeClaim, name: multi-backup-data}}
    patch: |-
      - op: replace
        path: /spec/resources/requests/storage
        value: {storage}
"#,base=base.display());
    fs::write(out.join("kustomization.yaml"),patch)?;
    println!("Created {}",out.display());
    println!("Next: create Secret multi-backup-auth and TLS Secret, inspect kubectl kustomize {}, then apply.",out.display());
    Ok(())
}
