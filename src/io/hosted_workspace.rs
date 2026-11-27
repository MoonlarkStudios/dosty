//! Client for versioned hosted Markdown workspaces.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::{Method, StatusCode, Url};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::workspace_document::{Cover, Details};

const API: &str = "/api/workspace/v1";
const MAX_RESPONSE: usize = 8 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct Client {
    http: reqwest::Client,
    origin: Url,
    key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Action {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    pub(crate) version: u32,
    pub(crate) name: String,
    pub(crate) actions: Vec<Action>,
    #[serde(default)]
    pub(crate) sections: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RequestError {
    status: Option<StatusCode>,
    pub(crate) message: String,
}

impl RequestError {
    pub(crate) fn conflict(&self) -> bool {
        self.status == Some(StatusCode::CONFLICT)
    }
}

impl From<String> for RequestError {
    fn from(message: String) -> Self {
        Self {
            status: None,
            message,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub(crate) id: uuid::Uuid,
    pub(crate) revision: u64,
    pub(crate) published_revision: Option<u64>,
    pub(crate) actions: Vec<Action>,
    #[serde(flatten)]
    pub(crate) details: Details,
    #[serde(default)]
    pub(crate) body: String,
    pub(crate) publish: Option<Job>,
    pub(crate) preview: Option<Job>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum JobState {
    Queued,
    Building,
    Deploying,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub(crate) id: uuid::Uuid,
    pub(crate) post_id: uuid::Uuid,
    pub(crate) mode: String,
    pub(crate) state: JobState,
    pub(crate) revision: u64,
    pub(crate) url: Option<String>,
    pub(crate) error: Option<String>,
}

impl Job {
    pub(crate) fn active(&self) -> bool {
        matches!(
            self.state,
            JobState::Queued | JobState::Building | JobState::Deploying
        )
    }

    pub(crate) fn label(&self) -> &str {
        match (self.state, self.mode.as_str()) {
            (JobState::Succeeded, "preview") => "Preview ready",
            (JobState::Succeeded, _) => "Published",
            (JobState::Failed, _) => self.error.as_deref().unwrap_or("The job failed."),
            (_, "preview") => "Preparing preview…",
            _ => "Publishing…",
        }
    }
}

fn safe_origin(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Enter a workspace URL.".to_owned())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || !safe_scheme(&url)
    {
        return Err("Use an HTTPS workspace URL or a local preview URL.".into());
    }
    Ok(url)
}

fn safe_scheme(url: &Url) -> bool {
    url.host_str().is_some()
        && (url.scheme() == "https"
            || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1")))
}

pub(crate) fn open_result(value: &str) -> Result<(), String> {
    let url =
        Url::parse(value).map_err(|_| "The workspace returned an invalid link.".to_owned())?;
    if !safe_scheme(&url) || !url.username().is_empty() || url.password().is_some() {
        return Err("The workspace returned an invalid link.".into());
    }
    open::that_detached(url.as_str()).map_err(|error| error.to_string())
}

impl Client {
    pub(crate) fn new(origin: &str, key: String) -> Result<Self, String> {
        let origin = safe_origin(origin)?;
        if key.is_empty() {
            return Err("Enter the workspace access key.".into());
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { http, origin, key })
    }

    pub(crate) fn origin(&self) -> &str {
        self.origin.as_str()
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<(&str, Vec<u8>)>,
    ) -> Result<T, String> {
        self.request_checked(method, path, body)
            .await
            .map_err(|error| error.message)
    }

    async fn request_checked<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<(&str, Vec<u8>)>,
    ) -> Result<T, RequestError> {
        let mut url = self.origin.clone();
        url.set_path(&format!("{API}{path}"));
        let mut request = self.http.request(method, url).bearer_auth(&self.key);
        if let Some((content_type, body)) = body {
            request = request.header("Content-Type", content_type).body(body);
        }
        let mut response = request.send().await.map_err(|error| error.to_string())?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                return Err("The workspace response is too large.".to_owned().into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if !status.is_success() {
            let message = serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|value| value["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| format!("The workspace returned {status}."));
            return Err(RequestError {
                status: Some(status),
                message,
            });
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            "The workspace returned an unexpected response."
                .to_owned()
                .into()
        })
    }

    pub(crate) async fn workspace(&self) -> Result<Workspace, String> {
        let workspace: Workspace = self.request(Method::GET, "", None).await?;
        if workspace.version != 1 {
            return Err("This workspace uses a different protocol version.".into());
        }
        Ok(workspace)
    }

    pub(crate) async fn posts(&self) -> Result<Vec<Post>, String> {
        self.request(Method::GET, "/posts", None).await
    }

    pub(crate) async fn post(&self, id: uuid::Uuid) -> Result<Post, String> {
        self.request(Method::GET, &format!("/posts/{id}"), None)
            .await
    }

    pub(crate) async fn new_post(&self) -> Result<Post, String> {
        self.request(
            Method::POST,
            "/actions/new-post",
            Some(("application/json", b"{}".to_vec())),
        )
        .await
    }

    pub(crate) async fn copy_draft(
        &self,
        details: Details,
        source: &str,
    ) -> Result<Post, RequestError> {
        let data = serde_json::to_vec(&DraftFields {
            details,
            body: source,
        })
        .map_err(|error| error.to_string())?;
        self.request_checked(
            Method::POST,
            "/actions/new-post",
            Some(("application/json", data)),
        )
        .await
    }

    pub(crate) async fn save(
        &self,
        id: uuid::Uuid,
        revision: u64,
        details: Details,
        source: &str,
    ) -> Result<Post, RequestError> {
        #[derive(Serialize)]
        struct Save<'a> {
            revision: u64,
            #[serde(flatten)]
            draft: DraftFields<'a>,
        }
        let data = serde_json::to_vec(&Save {
            revision,
            draft: DraftFields {
                details,
                body: source,
            },
        })
        .map_err(|error| error.to_string())?;
        self.request_checked(
            Method::PUT,
            &format!("/posts/{id}"),
            Some(("application/json", data)),
        )
        .await
    }

    pub(crate) async fn action(
        &self,
        id: uuid::Uuid,
        revision: u64,
        action: &str,
    ) -> Result<Job, String> {
        let data = format!(r#"{{"revision":{revision}}}"#).into_bytes();
        self.request(
            Method::POST,
            &format!("/posts/{id}/actions/{action}"),
            Some(("application/json", data)),
        )
        .await
    }

    pub(crate) async fn job(&self, id: uuid::Uuid) -> Result<Job, String> {
        self.request(Method::GET, &format!("/jobs/{id}"), None)
            .await
    }

    pub(crate) async fn photo(&self) -> Result<Option<Cover>, String> {
        use tokio::io::AsyncReadExt;
        const LIMIT: u64 = 20 * 1024 * 1024;
        let Some(photo) = rfd::AsyncFileDialog::new()
            .set_title("Choose Cover Photo")
            .add_filter(
                "Photos",
                &["jpg", "jpeg", "png", "webp", "heic", "heif", "avif"],
            )
            .pick_file()
            .await
        else {
            return Ok(None);
        };
        let file = tokio::fs::File::open(photo.path())
            .await
            .map_err(|error| error.to_string())?;
        let mut data = Vec::new();
        file.take(LIMIT + 1)
            .read_to_end(&mut data)
            .await
            .map_err(|error| error.to_string())?;
        if data.len() as u64 > LIMIT {
            return Err("Choose a photo under 20 MiB.".into());
        }
        #[derive(Deserialize)]
        struct Uploaded {
            src: String,
        }
        let uploaded: Uploaded = self
            .request(
                Method::POST,
                "/media",
                Some(("application/octet-stream", data)),
            )
            .await?;
        Ok(Some(Cover {
            src: uploaded.src,
            alt: String::new(),
        }))
    }
}

#[derive(Serialize)]
struct DraftFields<'a> {
    #[serde(flatten)]
    details: Details,
    body: &'a str,
}
