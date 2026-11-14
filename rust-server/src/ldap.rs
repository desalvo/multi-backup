//! Optional LDAPS authentication. Never allow plaintext LDAP or broad group
//! membership by default: an explicit allowed-users list or group DN is required.
use ldap3::{dn_escape, ldap_escape, LdapConn, LdapConnSettings, Scope};
use std::{collections::HashSet,env,time::Duration};
use zeroize::Zeroizing;
type Res<T>=Result<T,Box<dyn std::error::Error+Send+Sync>>;
#[derive(Clone)]
pub struct LdapConfig{
    url:String,
    user_dn_template:String,
    group_dn:Option<String>,
    group_attribute:String,
    allowed:HashSet<String>
}
fn safe_user(s:&str)->bool{
    (3..=80).contains(&s.len())&&s.bytes().all(|b|
        b.is_ascii_alphanumeric()||b=='_' as u8||b==b'-'||b==b'.'||b==b'@')
}
impl LdapConfig{
    pub fn from_env()->Res<Option<Self>>{
        let url=env::var("MULTI_BACKUP_LDAP_URL").unwrap_or_default();
        if url.is_empty(){return Ok(None);}
        if !url.starts_with("ldaps://")||url.contains('@')||url.contains('#'){
            return Err("Only ldaps:// with verified TLS is accepted".into());
        }
        let template=env::var("MULTI_BACKUP_LDAP_USER_DN_TEMPLATE")?;
        if template.matches("{user}").count()!=1||template.len()>1024{
            return Err("LDAP user DN template must contain one {user}".into());
        }
        let group_dn=env::var("MULTI_BACKUP_LDAP_REQUIRED_GROUP_DN")
            .ok().filter(|v|!v.is_empty());
        let group_attribute=env::var("MULTI_BACKUP_LDAP_GROUP_MEMBER_ATTRIBUTE")
            .unwrap_or_else(|_|"member".into());
        if !matches!(group_attribute.as_str(),"member"|"uniqueMember"|"memberUid") {
            return Err("LDAP group membership attribute must be recognized".into());
        }
        let allowed:HashSet<String>=env::var("MULTI_BACKUP_LDAP_ALLOWED_USERS")
            .unwrap_or_default().split(',').map(str::trim)
            .filter(|v|!v.is_empty()).map(str::to_string).collect();
        if allowed.iter().any(|x|!safe_user(x)) ||
           (allowed.is_empty()&&group_dn.is_none()){
            return Err("LDAPS access requires an explicit allow-list or required group".into());
        }
        if group_dn.as_ref().is_some_and(|x|x.len()>1024||x.contains('\0')){
            return Err("Invalid LDAP group DN".into());
        }
        Ok(Some(Self{url,user_dn_template:template,group_dn,
            group_attribute,allowed}))
    }
    pub fn authenticate(&self,user:&str,password:&str)->Res<bool>{
        if !safe_user(user)||password.is_empty()||password.len()>1024 {
            return Ok(false);
        }
        if !self.allowed.is_empty()&&!self.allowed.contains(user){return Ok(false);}
        let dn=self.user_dn_template.replace("{user}",&dn_escape(user));
        let settings=LdapConnSettings::new()
            .set_conn_timeout(Duration::from_secs(9))
            .set_no_tls_verify(false);
        let mut ldap=LdapConn::with_settings(settings,&self.url)?;
        let mut pwd=Zeroizing::new(password.to_string());
        let bind=ldap.simple_bind(&dn,pwd.as_str())?.success().is_ok();
        pwd.clear();
        if !bind{return Ok(false);}
        if let Some(ref group)=self.group_dn{
            let value=if self.group_attribute=="memberUid"{user}else{dn.as_str()};
            let filter=format!("({}={})",self.group_attribute,ldap_escape(value));
            let (found,_) = ldap.search(group,Scope::Base,&filter,vec!["dn"])?.success()?;
            if found.len()!=1 {return Ok(false);}
        }
        let _=ldap.unbind();
        Ok(true)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn rejects_ldap_injection_and_unsafe_names(){
        assert!(safe_user("alice@example.org"));
        assert!(!safe_user("*)(uid=*)"));
        assert!(!safe_user("john,ou=admins"));
        assert!(!safe_user(""));
        assert_eq!(ldap_escape("*)(user"),r"\2a\29\28user");
    }
}
