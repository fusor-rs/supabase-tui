use super::{
    Api, Error, ProjectRef,
    api::{Origin, accept, send},
};
use reqwest::{Method, RequestBuilder, Url};
use serde::Deserialize;
use serde_json::json;
use std::{path::Path, time::Duration};
use tokio::{fs::File, io::AsyncWriteExt};

/// The domain under which each project serves its own APIs.
const PROJECT_DOMAIN: &str = "supabase.co";
pub(crate) const SIGNED_URL_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// Auth takes a Go duration; about a hundred years bans an account for good.
const BAN_FOREVER: &str = "876000h";
const LIFT_BAN: &str = "none";

/// A project's Storage and Auth APIs, authorized with its secret key.
#[derive(Clone)]
pub(crate) struct ProjectApi {
    pub(crate) reference: ProjectRef,
    http: reqwest::Client,
    key: SecretKey,
}

/// Publishable-era secret keys go in `apikey` alone; the legacy `service_role` key is a JWT
/// that also goes in `Authorization`.
#[derive(Clone)]
enum SecretKey {
    Secret(String),
    ServiceRole(String),
}

/// A file in a storage bucket.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoredFile {
    pub(crate) bucket: String,
    pub(crate) path: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Ban {
    Forever,
    Lifted,
}

/// The emails Auth sends to a user on request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Email {
    Invitation,
    PasswordRecovery,
    MagicLink,
}

#[derive(Deserialize)]
struct ApiKey {
    name: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct Signed {
    #[serde(rename = "signedURL")]
    signed_url: String,
}

impl Api {
    /// The project's APIs, with its secret key revealed through the Management API.
    pub(crate) async fn project_api(&self, project: &ProjectRef) -> Result<ProjectApi, Error> {
        let keys: Vec<ApiKey> = self
            .fetch(&format!("/projects/{project}/api-keys?reveal=true"))
            .await?;
        let secret = keys
            .iter()
            .filter(|key| key.kind.as_deref() == Some("secret"))
            .find_map(|key| key.api_key.clone())
            .map(SecretKey::Secret);
        let service_role = || {
            keys.iter()
                .filter(|key| key.name == "service_role")
                .find_map(|key| key.api_key.clone())
                .map(SecretKey::ServiceRole)
        };
        let key = secret.or_else(service_role).ok_or(Error::NoSecretKey)?;
        Ok(ProjectApi {
            reference: project.clone(),
            http: self.http(),
            key,
        })
    }
}

impl ProjectApi {
    /// A link to `file` that works without a key until it expires.
    pub(crate) async fn signed_url(&self, file: &StoredFile) -> Result<String, Error> {
        let url = self.object_url(&["storage", "v1", "object", "sign"], file);
        let request = self
            .request(Method::POST, url)
            .json(&json!({ "expiresIn": SIGNED_URL_LIFETIME.as_secs() }));
        let signed: Signed = serde_json::from_str(&send(request, Origin::Project).await?)?;
        Ok(format!("{}/storage/v1{}", self.origin(), signed.signed_url))
    }

    /// Saves `file` at `destination`, which must not exist yet; returns its size in bytes.
    pub(crate) async fn download(
        &self,
        file: &StoredFile,
        destination: &Path,
    ) -> Result<u64, Error> {
        let url = self.object_url(&["storage", "v1", "object", "authenticated"], file);
        let request = self.request(Method::GET, url);
        let destination = destination.to_owned();
        tokio::spawn(async move {
            let mut response = accept(request.send().await?, Origin::Project).await?;
            let mut output = File::create_new(&destination).await?;
            let mut size = 0;
            while let Some(chunk) = response.chunk().await? {
                output.write_all(&chunk).await?;
                size += chunk.len() as u64;
            }
            output.flush().await?;
            Ok(size)
        })
        .await?
    }

    /// The first `limit` bytes of `file`, and whether there is more.
    pub(crate) async fn head(
        &self,
        file: &StoredFile,
        limit: usize,
    ) -> Result<(Vec<u8>, bool), Error> {
        let url = self.object_url(&["storage", "v1", "object", "authenticated"], file);
        let request = self.request(Method::GET, url);
        tokio::spawn(async move {
            let mut response = accept(request.send().await?, Origin::Project).await?;
            let mut head = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                head.extend_from_slice(&chunk);
                if head.len() > limit {
                    head.truncate(limit);
                    return Ok((head, true));
                }
            }
            Ok((head, false))
        })
        .await?
    }

    pub(crate) async fn send_email(&self, email: Email, address: &str) -> Result<(), Error> {
        let endpoint = match email {
            Email::Invitation => "invite",
            Email::PasswordRecovery => "recover",
            Email::MagicLink => "magiclink",
        };
        let url = self.url(&["auth", "v1", endpoint]);
        let request = self
            .request(Method::POST, url)
            .json(&json!({ "email": address }));
        send(request, Origin::Project).await?;
        Ok(())
    }

    pub(crate) async fn ban(&self, user: &str, ban: Ban) -> Result<(), Error> {
        let duration = match ban {
            Ban::Forever => BAN_FOREVER,
            Ban::Lifted => LIFT_BAN,
        };
        let url = self.url(&["auth", "v1", "admin", "users", user]);
        let request = self
            .request(Method::PUT, url)
            .json(&json!({ "ban_duration": duration }));
        send(request, Origin::Project).await?;
        Ok(())
    }

    pub(crate) async fn delete_user(&self, user: &str) -> Result<(), Error> {
        let url = self.url(&["auth", "v1", "admin", "users", user]);
        send(self.request(Method::DELETE, url), Origin::Project).await?;
        Ok(())
    }

    fn origin(&self) -> String {
        format!("https://{}.{PROJECT_DOMAIN}", self.reference)
    }

    /// The URL of `segments` under the project, each percent-encoded.
    fn url(&self, segments: &[&str]) -> Url {
        let mut url = Url::parse(&self.origin()).expect("a project reference forms a valid host");
        url.path_segments_mut()
            .expect("an https URL has a path")
            .extend(segments);
        url
    }

    fn object_url(&self, route: &[&str], file: &StoredFile) -> Url {
        let mut segments = route.to_vec();
        segments.push(&file.bucket);
        segments.extend(file.path.split('/'));
        self.url(&segments)
    }

    fn request(&self, method: Method, url: Url) -> RequestBuilder {
        let request = self.http.request(method, url);
        match &self.key {
            SecretKey::Secret(key) => request.header("apikey", key),
            SecretKey::ServiceRole(key) => request.header("apikey", key).bearer_auth(key),
        }
    }
}
