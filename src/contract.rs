//! The wire contract: header names, the event list, and the path a consumer
//! exposes. This module is the single definition of those strings. indiefuture
//! itself depends on this crate for them, so a server and a consumer built
//! against the same version cannot disagree.

/// `t=<unix seconds>,v1=<hex HMAC-SHA256 of "t.body">`, the way Stripe signs.
pub const SIGNATURE_HEADER: &str = "Indiefuture-Signature";
/// The event's type, repeated out of the body for cheap routing.
pub const EVENT_HEADER: &str = "Indiefuture-Event";
/// The delivery's id, the same on every attempt: use it as an idempotency key.
pub const DELIVERY_HEADER: &str = "Indiefuture-Delivery";

/// Where a consumer receives deliveries, appended to its own base URL.
pub const WEBHOOK_PATH: &str = "/api/v1/webhooks/indiefuture";

/// How far a delivery's timestamp may be from the receiver's clock.
pub const SIGNATURE_TOLERANCE_SECS: i64 = 300;

pub struct EventDef {
    pub name: &'static str,
    pub description: &'static str,
}

pub const TEST_EVENT: &str = "webhook.test";

pub const EVENTS: [EventDef; 10] = [
    EventDef { name: "episode.ready", description: "An episode's playback files are ready, after an upload, an import or a live recording." },
    EventDef { name: "episode.failed", description: "Processing an episode failed; `data.error` says why." },
    EventDef { name: "transcript.ready", description: "A transcript was saved; `data.version` is its version." },
    EventDef {
        name: "text.ready",
        description: "Written text was saved: `data.kind` (episode_pack, social, newsletter, blog or clips) and `data.version`.",
    },
    EventDef { name: "clip.rendered", description: "A clip's video is ready: `data.clip_id`." },
    EventDef { name: "episode.published", description: "An episode was published, or scheduled: `data.published_at`." },
    EventDef { name: "episode.unpublished", description: "An episode was taken off its show page and feed." },
    EventDef {
        name: "live.started",
        description: "A show went on air: `data.show_id`, `data.broadcast_id`, `data.title`, `data.started_at`, `data.restricted`.",
    },
    EventDef {
        name: "live.ended",
        description: "A broadcast went off air: `data.show_id`, `data.broadcast_id`, `data.ended_at`. Its recording follows as `episode.ready`.",
    },
    EventDef { name: TEST_EVENT, description: "Sent by Test on an endpoint, whatever it subscribes to. Carries nothing." },
];

/// An event an endpoint can subscribe to (the test event always arrives).
pub fn subscribable(event: &str) -> bool {
    event != TEST_EVENT && EVENTS.iter().any(|e| e.name == event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_event_is_never_a_subscription() {
        assert!(subscribable("episode.ready") && subscribable("clip.rendered"));
        assert!(!subscribable(TEST_EVENT) && !subscribable("episode.deleted"));
    }
}
