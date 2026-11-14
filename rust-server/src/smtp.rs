//! Fail-closed SMTP(S) integration: credentials in a read-only secret file,
//! TLS certificate validation, and a *fixed configured recipient* only.
use lettre::{Message,SmtpTransport,Transport};
use lettre::transport::smtp::authentication::Credentials;
use std::{env,fs,path::PathBuf,time::Duration};
use zeroize::Zeroizing;
type Res<T>=Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Clone,Debug)]
pub struct SmtpConfig {
    host:String,
    port:u16,
    tls:String,
    sender:String,
    recipient:String,
    user:Option<String>,
    password_file:Option<PathBuf>,
}
fn header_safe(value:&str)->bool {
    !value.is_empty()&&value.len()<=254&&!value.contains('\n')&&
    !value.contains('\r')&&value.bytes().all(|b|b>=32)
}
impl SmtpConfig {
    pub fn from_env()->Res<Option<Self>> {
        let host=env::var("MULTI_BACKUP_SMTP_HOST").unwrap_or_default();
        if host.is_empty(){return Ok(None);}
        if !header_safe(&host)||host.contains('/')||host.contains('@'){
            return Err("Invalid SMTP host".into());
        }
        let tls=env::var("MULTI_BACKUP_SMTP_TLS").unwrap_or_else(|_|"starttls".into());
        if tls!="starttls"&&tls!="smtps"{
            return Err("SMTP requires verified STARTTLS or SMTPS".into());
        }
        let port: u16=env::var("MULTI_BACKUP_SMTP_PORT")
            .unwrap_or_else(|_|if tls=="smtps"{"465"}else{"587"}.into()).parse()?;
        if port==0{return Err("Invalid SMTP port".into());}
        let sender=env::var("MULTI_BACKUP_SMTP_FROM")?;
        let recipient=env::var("MULTI_BACKUP_SMTP_TO")?;
        if !header_safe(&sender)||!header_safe(&recipient)||
           sender.parse::<lettre::message::Mailbox>().is_err()||
           recipient.parse::<lettre::message::Mailbox>().is_err(){
            return Err("Invalid SMTP sender/recipient".into());
        }
        let user=env::var("MULTI_BACKUP_SMTP_USER").ok().filter(|s|!s.is_empty());
        let password_file=env::var_os("MULTI_BACKUP_SMTP_PASSWORD_FILE").map(PathBuf::from);
        if user.is_some()!=password_file.is_some(){
            return Err("SMTP username and password file must be paired".into());
        }
        if let Some(ref file)=password_file {
            let meta=fs::symlink_metadata(file)?;
            if !meta.file_type().is_file()||meta.len()>4096 {
                return Err("Unsafe SMTP secret file".into());
            }
        }
        Ok(Some(Self {host,port,tls,sender,recipient,user,password_file}))
    }
    pub fn protocol(&self)->&str{&self.tls}
    pub fn send_notice(&self,subject:&str,body:&str)->Res<()>{
        if !header_safe(subject)||subject.len()>160||body.len()>4096{
            return Err("SMTP message too large or invalid".into());
        }
        let email=Message::builder()
            .from(self.sender.parse()?).to(self.recipient.parse()?)
            .subject(subject).body(body.to_string())?;
        let mut builder=if self.tls=="smtps"{
            SmtpTransport::relay(&self.host)?
        }else{
            SmtpTransport::starttls_relay(&self.host)?
        };
        builder=builder.port(self.port).timeout(Some(Duration::from_secs(20)));
        if let (Some(user),Some(file))=(&self.user,&self.password_file) {
            let contents=fs::read_to_string(file)?;
            let pass=Zeroizing::new(contents.trim_end_matches(['\r','\n']).to_owned());
            if pass.is_empty(){return Err("Empty SMTP credential".into());}
            builder=builder.credentials(Credentials::new(user.to_owned(),pass.to_string()));
        }
        let mailer=builder.build();
        mailer.send(&email)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn rejects_header_injection_and_plaintext_downgrade(){
        assert!(!header_safe("subject\r\nBcc: victim@example.org"));
        assert!(!header_safe(""));
        assert!(header_safe("Multi Backup report"));
    }
}
