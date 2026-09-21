# indiefuture-client

Rust client for [indiefuture](https://indiefuture.com): the API a workspace is
driven through, and the signed webhooks it sends back.

This crate is the single definition of the wire contract — header names, the
event list, the signature scheme. indiefuture itself depends on it for those,
so a server and a consumer built against the same version cannot drift apart.

```toml
[dependencies]
indiefuture-client = { git = "https://github.com/ethereumdegen/indiefuture-client-rs" }
```

## Calling the API

```rust
use indiefuture_client::{Client, Config};

let http = reqwest::Client::new();
let config = Config {
    base_url: "https://indiefuture.com".into(),
    api_key: std::env::var("INDIEFUTURE_API_KEY")?,
    workspace_id: std::env::var("INDIEFUTURE_WORKSPACE_ID")?.parse()?,
    webhook_secret: std::env::var("INDIEFUTURE_WEBHOOK_SECRET").ok(),
};
let indiefuture = Client::new(&http, &config);

let show = indiefuture.create_show("Sunday circle", "Weekly", "video").await?;
let episode = indiefuture.create_episode(show, "Episode 1", "").await?;
indiefuture.send_recording(episode, &signed_download_url, "recording.mp4").await?;
```

Reads: `shows`, `episodes`, `episode`, `transcript`, `notes`. Live: `create_live_show`,
`arm`, `disarm`, `stream_key`, `live`, `live_playback`, `episode_playback`, `restrict_show`.
Anything not wrapped goes through `call`/`ok`, which return indiefuture's status and body as they are.

`EpisodeDetail::audio(with_ads)` picks the right file: the workspace's ad copy
when it exists, the clean one otherwise, and the plain mp3 on an indiefuture
older than S17.

## Receiving webhooks

A delivery is a POST of `{id, type, created_at, workspace_id, data}` carrying
`Indiefuture-Signature: t=<unix seconds>,v1=<hex HMAC-SHA256 of "t.body">`.

```rust
use indiefuture_client::webhook;

let body = webhook::verify_any(&raw_body, signature_header, &secrets, now_unix)?;
```

`verify_any` takes the secret held for the registered endpoint plus any
configured fallback, so a rotation does not drop deliveries. It refuses a
signature more than five minutes from your clock and compares digests in
constant time. Treat the body's `id` as the idempotency key: a retried delivery
carries the same one.

Register the endpoint with the API key alone, no dashboard trip:

```rust
let (connected, new_secret) = indiefuture
    .ensure_webhook(&webhook_url(&my_base_url), "My site (registered by the site)", held_endpoint_id)
    .await?;
```

It creates the endpoint when none points at you, enables a disabled one, and
rotates the secret when an endpoint exists but you do not hold its secret —
the only moment a secret is readable, so store it when it comes back. Needs a
write-scoped key and is safe to run on every boot.

## Compatibility

The crate deserializes what an older indiefuture sends: fields a server may
omit are optional or defaulted. Bump the git dependency when you need an
endpoint added in a later release.
