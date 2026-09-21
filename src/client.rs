//! The HTTP client. Every call goes to one workspace, authenticated with that
//! workspace's API key (`Authorization: Bearer sck_…`).

use bytes::Bytes;
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    Error, Result,
    model::{
        Connected, Created, Endpoint, EpisodeDetail, EpisodePlayback, EpisodeSummary, LivePlayback, LiveView, Notes, Segment, Show, StreamKey,
        parse_notes,
    },
};

/// Where indiefuture is, and the key to reach one workspace on it. The key and
/// the webhook secret are kept out of `Debug`.
#[derive(Clone)]
pub struct Config {
    /// indiefuture's origin, without a trailing slash.
    pub base_url: String,
    pub api_key: String,
    pub workspace_id: Uuid,
    /// A secret to accept deliveries with, besides the registered endpoint's.
    pub webhook_secret: Option<String>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("workspace_id", &self.workspace_id)
            .field("api_key", &"…")
            .field("webhook_secret", &self.webhook_secret.as_ref().map(|_| "…"))
            .finish()
    }
}

pub struct Client<'a> {
    http: &'a reqwest::Client,
    config: &'a Config,
}

impl<'a> Client<'a> {
    pub fn new(http: &'a reqwest::Client, config: &'a Config) -> Self {
        Self { http, config }
    }

    pub fn config(&self) -> &Config {
        self.config
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1/workspaces/{}{path}", self.config.base_url.trim_end_matches('/'), self.config.workspace_id)
    }

    /// A call, with indiefuture's status and body exactly as they came.
    pub async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Result<(StatusCode, Value)> {
        let mut request = self.http.request(method, self.url(path)).bearer_auth(&self.config.api_key);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        Ok((status, response.json::<Value>().await.unwrap_or(Value::Null)))
    }

    /// A call that must succeed.
    pub async fn ok(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        let (status, body) = self.call(method, path, body).await?;
        if !status.is_success() {
            return Err(Error::api(status, &body));
        }
        Ok(body)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(serde_json::from_value(self.ok(Method::GET, path, None).await?)?)
    }

    // Shows and episodes ----------------------------------------------------

    pub async fn shows(&self) -> Result<Vec<Show>> {
        self.get("/shows").await
    }

    pub async fn episodes(&self, show_id: Uuid) -> Result<Vec<EpisodeSummary>> {
        self.get(&format!("/shows/{show_id}/episodes")).await
    }

    /// An episode, or `None` once it is gone.
    pub async fn episode(&self, episode_id: Uuid) -> Result<Option<EpisodeDetail>> {
        let (status, body) = self.call(Method::GET, &format!("/episodes/{episode_id}"), None).await?;
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(Error::api(status, &body));
        }
        Ok(Some(serde_json::from_value(body)?))
    }

    pub async fn create_show(&self, title: &str, description: &str, medium: &str) -> Result<Uuid> {
        let body = json!({ "title": title, "description": description, "medium": medium });
        Ok(serde_json::from_value::<Created>(self.ok(Method::POST, "/shows", Some(body)).await?)?.id)
    }

    pub async fn create_episode(&self, show_id: Uuid, title: &str, description: &str) -> Result<Uuid> {
        let body = json!({ "title": title, "description": description });
        let made = self.ok(Method::POST, &format!("/shows/{show_id}/episodes"), Some(body)).await?;
        Ok(serde_json::from_value::<Created>(made)?.id)
    }

    /// Hand indiefuture the episode's file by link. Already receiving it (409)
    /// counts as done, so this is safe to retry.
    pub async fn send_recording(&self, episode_id: Uuid, url: &str, filename: &str) -> Result<()> {
        let body = json!({ "url": url, "filename": filename });
        let (status, answer) = self.call(Method::POST, &format!("/episodes/{episode_id}/source/url"), Some(body)).await?;
        if status.is_success() || status == StatusCode::CONFLICT {
            return Ok(());
        }
        Err(Error::api(status, &answer))
    }

    pub async fn transcript(&self, episode_id: Uuid) -> Result<Vec<Segment>> {
        let body = self.ok(Method::GET, &format!("/episodes/{episode_id}/transcript"), None).await?;
        let segments = body.pointer("/transcript/segments").cloned().ok_or(Error::NoSegments)?;
        Ok(serde_json::from_value(segments)?)
    }

    pub async fn notes(&self, episode_id: Uuid) -> Result<Notes> {
        Ok(parse_notes(&self.ok(Method::GET, &format!("/episodes/{episode_id}/text"), None).await?))
    }

    /// Replace an ad slot's audio: the file is the request body.
    pub async fn put_ad_audio(&self, ad_id: Uuid, content_type: &str, audio: Bytes) -> Result<(StatusCode, Value)> {
        let response = self
            .http
            .put(self.url(&format!("/ads/{ad_id}/audio")))
            .bearer_auth(&self.config.api_key)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .timeout(std::time::Duration::from_secs(120))
            .body(audio)
            .send()
            .await?;
        let status = response.status();
        Ok((status, response.json::<Value>().await.unwrap_or(Value::Null)))
    }

    // Live ------------------------------------------------------------------

    /// Make a show members-only: its streams, recordings and published
    /// episodes play only through links the consumer asks for. Safe to repeat.
    pub async fn restrict_show(&self, show_id: Uuid) -> Result<()> {
        self.ok(Method::PUT, &format!("/shows/{show_id}/live/access"), Some(json!({ "access": "restricted" }))).await?;
        Ok(())
    }

    /// A show that holds a live input, restricted from the start.
    pub async fn create_live_show(&self, title: &str, description: &str) -> Result<Uuid> {
        let show = self.create_show(title, description, "video").await?;
        self.restrict_show(show).await?;
        Ok(show)
    }

    pub async fn live(&self, show_id: Uuid) -> Result<LiveView> {
        self.get(&format!("/shows/{show_id}/live")).await
    }

    /// Arm the show's input for OBS; `title` names the recording.
    pub async fn arm(&self, show_id: Uuid, title: &str) -> Result<LiveView> {
        let body = self.ok(Method::POST, &format!("/shows/{show_id}/live/arm"), Some(json!({ "title": title }))).await?;
        Ok(serde_json::from_value(body)?)
    }

    pub async fn disarm(&self, show_id: Uuid) -> Result<LiveView> {
        let body = self.ok(Method::POST, &format!("/shows/{show_id}/live/disarm"), Some(json!({}))).await?;
        Ok(serde_json::from_value(body)?)
    }

    /// The RTMPS server and stream key for OBS; `rotate` makes a new key and
    /// drops whatever encoder is on the old one.
    pub async fn stream_key(&self, show_id: Uuid, rotate: bool) -> Result<StreamKey> {
        let path = format!("/shows/{show_id}/live/key{}", if rotate { "/rotate" } else { "" });
        Ok(serde_json::from_value(self.ok(Method::POST, &path, Some(json!({}))).await?)?)
    }

    /// A signed link to the show's stream, lasting `ttl_seconds`.
    pub async fn live_playback(&self, show_id: Uuid, ttl_seconds: i64) -> Result<LivePlayback> {
        let body = self.ok(Method::POST, &format!("/shows/{show_id}/live/playback"), Some(json!({ "ttl_seconds": ttl_seconds }))).await?;
        Ok(serde_json::from_value(body)?)
    }

    /// A signed link to an episode's file, lasting `ttl_seconds`.
    pub async fn episode_playback(&self, episode_id: Uuid, ttl_seconds: i64) -> Result<EpisodePlayback> {
        let body = self.ok(Method::POST, &format!("/episodes/{episode_id}/playback"), Some(json!({ "ttl_seconds": ttl_seconds }))).await?;
        Ok(serde_json::from_value(body)?)
    }

    // Webhooks --------------------------------------------------------------

    pub async fn endpoints(&self) -> Result<Vec<Endpoint>> {
        self.get("/webhooks").await
    }

    /// Make sure the workspace has an enabled webhook pointing at `url`.
    ///
    /// `held` is the endpoint whose secret the caller already holds. Returns
    /// what happened and, when one had to be made or rotated, the endpoint and
    /// its new secret — the only time a secret can be read. Needs a write key,
    /// and is safe to run any number of times.
    pub async fn ensure_webhook(&self, url: &str, description: &str, held: Option<Uuid>) -> Result<(Connected, Option<(Uuid, String)>)> {
        let wanted = url.trim_end_matches('/');
        let Some(endpoint) = self.endpoints().await?.into_iter().find(|e| e.url.trim_end_matches('/') == wanted) else {
            let body = json!({ "url": url, "description": description, "events": [] });
            let made = self.ok(Method::POST, "/webhooks", Some(body)).await?;
            let id = made.pointer("/endpoint/id").cloned().ok_or(Error::Missing("endpoint id"))?;
            let secret = made["secret"].as_str().ok_or(Error::Missing("secret"))?;
            return Ok((Connected::Created, Some((serde_json::from_value(id)?, secret.to_owned()))));
        };
        if !endpoint.enabled {
            self.ok(Method::PATCH, &format!("/webhooks/{}", endpoint.id), Some(json!({ "enabled": true }))).await?;
        }
        if held == Some(endpoint.id) {
            return Ok((Connected::AlreadyConnected, None));
        }
        // Someone made it by hand; its secret cannot be read back, so take a new one.
        let rotated = self.ok(Method::POST, &format!("/webhooks/{}/secret", endpoint.id), None).await?;
        let secret = rotated["secret"].as_str().ok_or(Error::Missing("secret"))?;
        Ok((Connected::Rotated, Some((endpoint.id, secret.to_owned()))))
    }
}
