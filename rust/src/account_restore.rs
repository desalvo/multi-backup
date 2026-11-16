//! Provider-independent cross-account restore intent and fail-closed planning.
//! Credentials belong to the destination login, never to a backup manifest.
use crate::plugins::{self,Mode};
use serde::{Serialize,Deserialize};

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct AccountRestorePlan {
    pub provider: String,
    /// Origin is historical provenance, NOT authorization to use credentials.
    pub source_account: String,
    /// Explicit destination; may differ from origin.
    pub destination_account: String,
    pub selected_paths: Vec<String>,
    pub cross_account: bool,
    pub destination_authorization_required: bool,
    /// Generic executor is not implied by catalog/plan registration.
    pub executable: bool,
}

fn account(value:&str)->Result<(),String>{
    if value.is_empty() || value.len()>254 || value.trim()!=value ||
       value.chars().any(|c|c.is_control()) ||
       !value.bytes().all(|x|x.is_ascii_alphanumeric()||b"@._:-+".contains(&x)) {
        return Err("Invalid account identifier".into());
    }
    Ok(())
}
fn selection(path:&str)->bool {
    !path.is_empty() && path.len()<=4096 && !path.starts_with('/') &&
    !path.contains('\\') && !path.contains(':') &&
    !path.chars().any(|c|c.is_control()) &&
    path.split('/').all(|part|!part.is_empty()&&part!="."&&part!="..")
}
impl AccountRestorePlan {
    pub fn prepare(provider:&str,source_account:&str,destination_account:&str,
                   selected_paths:Vec<String>)->Result<Self,String>{
        account(source_account)?;
        account(destination_account)?;
        if selected_paths.len()>1000 || selected_paths.iter().any(|s|!selection(s)) {
            return Err("Invalid file/repository selection for cross-account restore".into());
        }
        let descriptor=plugins::find(provider).ok_or("Unknown provider")?;
        if !descriptor.output_stage(Mode::Restore).listed() {
            return Err("Provider does not support restoration into an account".into());
        }
        if !matches!(provider,"github"|"dropbox"|"google-drive"|"onedrive"|
                       "owncloud"|"nextcloud"|"alfresco") {
            return Err("Provider has no account-scoped restore contract".into());
        }
        let cross_account=!source_account.eq_ignore_ascii_case(destination_account);
        Ok(Self{provider:provider.into(),source_account:source_account.into(),
            destination_account:destination_account.into(),selected_paths,
            cross_account,destination_authorization_required:true,executable:false})
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn cross_account_is_first_class_but_does_not_bypass_a_provider(){
        let plan=AccountRestorePlan::prepare("github","oldowner","newowner",
            vec!["repo-a".into()]).unwrap();
        assert!(plan.cross_account);
        assert!(plan.destination_authorization_required);
        assert!(!plan.executable);
        assert!(AccountRestorePlan::prepare("dropbox","alice@example.com",
            "bob@example.com",vec!["docs/part.pdf".into()]).is_ok());
        for bad in ["../secret","/etc/passwd","a//b","a\\b","C:/windows"] {
            assert!(AccountRestorePlan::prepare("github","alice","bob",
                vec![bad.into()]).is_err());
        }
        assert!(AccountRestorePlan::prepare("postgresql","alice","bob",vec![]).is_err());
        assert!(AccountRestorePlan::prepare("github","bad\nname","bob",vec![]).is_err());
    }
}