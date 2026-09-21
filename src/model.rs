//! The parts of indiefuture's API answers that consumers read. Every field a
//! server may omit is optional or defaulted, so an older indiefuture still
//! deserializes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[cfg(feature = "client")]
#[derive(Debug, Deserialize)]
pub(crate) struct Created {
    pub id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Show {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpisodeSummary {
    pub id: Uuid,
    pub show_id: Uuid,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub medium: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub kind: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Playback {
    pub kind: Option<String>,
    pub src: Option<String>,
    pub poster: Option<String>,
}

/// Which audio to hand a listener: the clean file, or the same with the
/// workspace's ad slots stitched in once that copy is made. Absent on an
/// indiefuture older than S17.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Delivery {
    /// The listening file without inserted ads (enhanced when the host chose it).
    pub clean: Option<String>,
    /// The same file with the workspace's ad slots stitched in, once made.
    pub with_ads: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Publication {
    /// The show is restricted: the episode is on no public indiefuture page,
    /// feed or embed, and plays only through links the consumer asks for.
    #[serde(default)]
    pub members_only: bool,
}

/// One episode in full.
#[derive(Debug, Clone, Deserialize)]
pub struct EpisodeDetail {
    #[serde(flatten)]
    pub summary: EpisodeSummary,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub transcript_status: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub playback: Playback,
    #[serde(default)]
    pub publication: Publication,
    pub delivery: Option<Delivery>,
}

impl EpisodeDetail {
    pub fn id(&self) -> Uuid {
        self.summary.id
    }

    pub fn show_id(&self) -> Uuid {
        self.summary.show_id
    }

    pub fn asset(&self, kind: &str) -> Option<String> {
        self.assets.iter().find(|a| a.kind == kind).and_then(|a| a.url.clone())
    }

    /// The listening audio, with or without ads; the plain mp3 before S17.
    pub fn audio(&self, with_ads: bool) -> Option<String> {
        let delivery = self.delivery.clone().unwrap_or_default();
        let clean = delivery.clean.or_else(|| self.asset("audio_mp3"));
        if with_ads { delivery.with_ads.or(clean) } else { clean }
    }
}

/// A transcript segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub speaker: Option<String>,
    pub text: String,
}

/// The parts of an episode pack a consumer shows.
#[derive(Debug, Default, PartialEq)]
pub struct Notes {
    pub summary: Option<String>,
    /// Markdown with `[mm:ss]` timestamps.
    pub show_notes: Option<String>,
    /// `[{start_seconds, title}]`
    pub chapters: Option<Value>,
}

/// A show's live input and recent broadcasts.
#[derive(Debug, Clone, Deserialize)]
pub struct LiveView {
    pub input: Option<LiveInput>,
    #[serde(default)]
    pub broadcasts: Vec<Broadcast>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LiveInput {
    pub armed: bool,
    /// idle, connected, reconnecting or errored.
    pub connection: String,
    pub on_air: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Broadcast {
    pub id: Uuid,
    /// live, processing, downloading, imported or failed.
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub duration_ms: Option<i64>,
    pub episode_id: Option<Uuid>,
    /// The recording's episode: processing until its files are ready.
    pub episode_status: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamKey {
    pub rtmps_url: String,
    pub stream_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LivePlayback {
    pub on_air: bool,
    pub broadcast_id: Option<Uuid>,
    pub hls_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EpisodePlayback {
    pub url: String,
}

/// A webhook endpoint registered on the workspace.
#[derive(Debug, Clone, Deserialize)]
pub struct Endpoint {
    pub id: Uuid,
    pub url: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

/// What [`crate::Client::ensure_webhook`] found or did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connected {
    /// Already registered, and the caller holds its secret.
    AlreadyConnected,
    /// No endpoint pointed at the caller: made one.
    Created,
    /// An endpoint pointed at the caller but it did not hold the secret: rotated it.
    Rotated,
}

/// Read the episode pack out of a `/text` answer. [`crate::Client::notes`]
/// calls this; it is public so a stored answer can be re-read without a call.
pub fn parse_notes(body: &Value) -> Notes {
    let pack = body["kinds"]
        .as_array()
        .and_then(|kinds| kinds.iter().find(|kind| kind["kind"] == "episode_pack"))
        .and_then(|kind| kind.pointer("/latest/content"));
    let text = |field: &str| pack.and_then(|p| p[field].as_str()).map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned);
    Notes {
        summary: text("summary"),
        show_notes: text("show_notes"),
        chapters: pack.and_then(|p| p.get("chapters")).filter(|c| c.as_array().is_some_and(|a| !a.is_empty())).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn episode(delivery: Value, assets: Value) -> EpisodeDetail {
        serde_json::from_value(json!({
            "id": Uuid::nil(), "show_id": Uuid::nil(), "slug": "s", "title": "t", "status": "ready",
            "assets": assets, "delivery": delivery,
        }))
        .unwrap()
    }

    #[test]
    fn audio_prefers_the_delivery_copy_and_falls_back_to_the_plain_mp3() {
        let assets = json!([{ "kind": "audio_mp3", "url": "https://x/plain.mp3" }]);

        let both = episode(json!({ "clean": "https://x/clean.mp3", "with_ads": "https://x/ads.mp3" }), assets.clone());
        assert_eq!(both.audio(false).as_deref(), Some("https://x/clean.mp3"));
        assert_eq!(both.audio(true).as_deref(), Some("https://x/ads.mp3"));

        // The ad copy is not made yet: a listener still gets the clean one.
        let clean_only = episode(json!({ "clean": "https://x/clean.mp3" }), assets.clone());
        assert_eq!(clean_only.audio(true).as_deref(), Some("https://x/clean.mp3"));

        // An indiefuture older than S17 sends no delivery at all.
        let old = episode(Value::Null, assets);
        assert_eq!(old.audio(true).as_deref(), Some("https://x/plain.mp3"));

        let nothing = episode(Value::Null, json!([]));
        assert_eq!(nothing.audio(false), None);
    }

    #[test]
    fn an_episode_deserializes_without_the_fields_a_server_may_omit() {
        let bare: EpisodeDetail = serde_json::from_value(json!({
            "id": Uuid::nil(), "show_id": Uuid::nil(), "slug": "s", "title": "t", "status": "processing"
        }))
        .unwrap();
        assert_eq!((bare.transcript_status.as_str(), bare.publication.members_only, bare.assets.len()), ("", false, 0));
        assert_eq!(bare.summary.duration_ms, None);
    }

    #[test]
    fn notes_are_read_from_the_episode_pack_and_blank_fields_are_dropped() {
        let body = json!({ "kinds": [
            { "kind": "social", "latest": { "content": { "summary": "wrong" } } },
            { "kind": "episode_pack", "latest": { "content": {
                "summary": "  A quick hello.  ", "show_notes": "   ", "chapters": [{ "start_seconds": 0, "title": "Welcome" }]
            } } },
        ] });
        let notes = parse_notes(&body);
        assert_eq!(notes.summary.as_deref(), Some("A quick hello."));
        assert_eq!(notes.show_notes, None);
        assert_eq!(notes.chapters.unwrap()[0]["title"], "Welcome");

        // No pack yet, and an empty chapter list, both read as nothing.
        assert_eq!(parse_notes(&json!({ "kinds": [] })), Notes::default());
        assert_eq!(parse_notes(&json!({ "kinds": [{ "kind": "episode_pack", "latest": { "content": { "chapters": [] } } }] })).chapters, None);
    }
}
