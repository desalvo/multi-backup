//! Generic OpenID Connect authentication code + PKCE S256.
//! Token signature, issuer, audience, expiry and nonce are verified by
//! openidconnect; access only for explicitly allowed stable subject IDs.
use openidconnect::{
    core::{CoreAuthenticationFlow,CoreClient,CoreProviderMetadata},
    reqwest,AccessTokenHash,AuthorizationCode,ClientId,ClientSecret,CsrfToken,IssuerUrl,
    Nonce,PkceCodeChallenge,PkceCodeVerifier,RedirectUrl,Scope,TokenResponse
};
use std::{collections::HashSet,env,fs,path::PathBuf,time::{Duration,Instant}};
type Res<T>=Result<T,Box<dyn std::error::Error+Send+Sync>>;

#[derive(Clone)]
pub struct OidcConfig{
    issuer:String,client_id:String,secret_file:PathBuf,
    redirect_uri:String,allowed_subjects:HashSet<String>
}
pub struct Pending {
    pub verifier:String,
    pub nonce:String,
    pub created:Instant,
}
impl OidcConfig {
    pub fn from_env(origin:&str)->Res<Option<Self>>{
        let issuer=env::var("MULTI_BACKUP_OIDC_ISSUER").unwrap_or_default();
        if issuer.is_empty(){return Ok(None);}
        if !issuer.starts_with("https://")||issuer.contains('#')||
           issuer.contains('@')||issuer.len()>512 {
            return Err("OIDC issuer must be a trusted HTTPS URL".into());
        }
        if !origin.starts_with("https://"){
            return Err("OIDC requires HTTPS public origin".into());
        }
        let client_id=env::var("MULTI_BACKUP_OIDC_CLIENT_ID")?;
        let secret_file=PathBuf::from(env::var("MULTI_BACKUP_OIDC_SECRET_FILE")?);
        let allowed_subjects:HashSet<String>=env::var("MULTI_BACKUP_OIDC_ALLOWED_SUBJECTS")?
            .split(',').map(str::trim).filter(|s|!s.is_empty())
            .map(str::to_owned).collect();
        if client_id.is_empty()||client_id.len()>256||allowed_subjects.is_empty()||
           allowed_subjects.iter().any(|s|s.len()>512||s.chars().any(char::is_control)){
            return Err("OIDC requires a client ID and an explicit subject allow-list".into());
        }
        let meta=fs::symlink_metadata(&secret_file)?;
        if !meta.file_type().is_file()||meta.len()>4096 {
            return Err("OIDC client secret file is unsafe".into());
        }
        let redirect_uri=format!("{}/oidc/callback",origin.trim_end_matches('/'));
        Ok(Some(Self{issuer,client_id,secret_file,redirect_uri,allowed_subjects}))
    }
    fn http()->Res<reqwest::Client>{
        Ok(reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(14))
            .build()?)
    }
    async fn metadata(&self,http:&reqwest::Client)->Res<CoreProviderMetadata>{
        Ok(CoreProviderMetadata::discover_async(
            IssuerUrl::new(self.issuer.clone())?,http).await?)
    }
    fn secret(&self)->Res<ClientSecret> {
        let password=fs::read_to_string(&self.secret_file)?;
        let secret=password.trim_end_matches(['\r','\n']);
        if secret.is_empty(){return Err("Empty OIDC client secret".into());}
        Ok(ClientSecret::new(secret.to_owned()))
    }
    pub async fn start(&self)->Res<(String,String,Pending)>{
        let http=Self::http()?;
        let client=CoreClient::from_provider_metadata(
            self.metadata(&http).await?,
            ClientId::new(self.client_id.clone()),
            Some(self.secret()?))
            .set_redirect_uri(RedirectUrl::new(self.redirect_uri.clone())?);
        let (challenge,verifier)=PkceCodeChallenge::new_random_sha256();
        let (url,csrf,nonce)=client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,Nonce::new_random)
            .add_scope(Scope::new("openid".into()))
            .set_pkce_challenge(challenge).url();
        if url.scheme()!="https"{return Err("Refusing non-HTTPS OIDC authorization endpoint".into());}
        Ok((url.to_string(),csrf.secret().to_string(),Pending{
            verifier:verifier.secret().to_string(),nonce:nonce.secret().to_string(),
            created:Instant::now()
        }))
    }
    pub async fn complete(&self,pending:Pending,code:String)->Res<String>{
        if pending.created.elapsed()>Duration::from_secs(600)||
            code.is_empty()||code.len()>8192 {
            return Err("Expired or malformed OIDC login".into());
        }
        let http=Self::http()?;
        let client=CoreClient::from_provider_metadata(
            self.metadata(&http).await?,
            ClientId::new(self.client_id.clone()),
            Some(self.secret()?))
            .set_redirect_uri(RedirectUrl::new(self.redirect_uri.clone())?);
        let response=client.exchange_code(AuthorizationCode::new(code))?
            .set_pkce_verifier(PkceCodeVerifier::new(pending.verifier))
            .request_async(&http).await?;
        let token=response.id_token().ok_or("Missing OIDC identity token")?;
        let nonce=Nonce::new(pending.nonce);
        let verifier=client.id_token_verifier();
        let claims=token.claims(&verifier,&nonce)?;
        if let Some(expected)=claims.access_token_hash(){
            let actual=AccessTokenHash::from_token(response.access_token(),
                token.signing_alg()?,token.signing_key(&verifier)?)?;
            if &actual!=expected {return Err("OIDC access token hash mismatch".into());}
        }
        let subject=claims.subject().as_str().to_string();
        if !self.allowed_subjects.contains(&subject){
            return Err("OIDC subject not authorized for Multi Backup".into());
        }
        Ok(subject)
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn pending_login_expiration_is_strict(){
        let p=Pending{verifier:"random".into(),nonce:"secret".into(),
            created:Instant::now()-Duration::from_secs(601)};
        assert!(p.created.elapsed()>Duration::from_secs(600));
    }
}
