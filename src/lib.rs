//! Rust client for [indiefuture](https://indiefuture.com): the API one
//! workspace is driven through, and the signed webhooks it sends back.
//!
//! This crate is the single definition of the wire contract. indiefuture itself
//! depends on it for the header names and the event list, so a server and a
//! consumer built against the same version cannot drift apart.
//!
//! ```no_run
//! use indiefuture_client::{Client, Config};
//! # async fn f() -> Result<(), Box<dyn std::error::Error>> {
//! let http = reqwest::Client::new();
//! let config = Config {
//!     base_url: "https://indiefuture.com".into(),
//!     api_key: std::env::var("INDIEFUTURE_API_KEY")?,
//!     workspace_id: std::env::var("INDIEFUTURE_WORKSPACE_ID")?.parse()?,
//!     webhook_secret: None,
//! };
//! let indiefuture = Client::new(&http, &config);
//! for show in indiefuture.shows().await? {
//!     println!("{} {}", show.id, show.title);
//! }
//! # Ok(()) }
//! ```
//!
//! Receiving a delivery: check [`webhook::verify_any`] against the secret held
//! for the registered endpoint, then treat the body's `id` as an idempotency
//! key — a retried delivery carries the same one.

pub mod client;
pub mod contract;
pub mod model;
pub mod webhook;

pub use client::{Client, Config};
pub use contract::{DELIVERY_HEADER, EVENT_HEADER, EVENTS, EventDef, SIGNATURE_HEADER, SIGNATURE_TOLERANCE_SECS, TEST_EVENT, WEBHOOK_PATH, subscribable};
pub use model::{
    Asset, Broadcast, Connected, Delivery, Endpoint, EpisodeDetail, EpisodePlayback, EpisodeSummary, LiveInput, LivePlayback, LiveView, Notes,
    Playback, Publication, Segment, Show, StreamKey,
};

/// What went wrong talking to indiefuture.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not reach indiefuture")]
    Transport(#[from] reqwest::Error),
    #[error("indiefuture answered {status}: {message}")]
    Api { status: reqwest::StatusCode, message: String },
    #[error("indiefuture's answer had no {0}")]
    Missing(&'static str),
    #[error("the transcript has no segments")]
    NoSegments,
    #[error("indiefuture's answer could not be read")]
    Decode(#[from] serde_json::Error),
}

impl Error {
    pub(crate) fn api(status: reqwest::StatusCode, body: &serde_json::Value) -> Self {
        let message = body.pointer("/error/message").and_then(serde_json::Value::as_str).unwrap_or("no message");
        Self::Api { status, message: message.to_owned() }
    }

    /// True when indiefuture refused the key or the workspace.
    pub fn is_auth(&self) -> bool {
        matches!(self, Self::Api { status, .. } if *status == reqwest::StatusCode::UNAUTHORIZED || *status == reqwest::StatusCode::FORBIDDEN)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Where a consumer receives deliveries, on its own origin.
pub fn webhook_url(base_url: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), WEBHOOK_PATH)
}
