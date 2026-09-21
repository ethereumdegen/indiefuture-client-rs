//! Signing and checking a delivery's `Indiefuture-Signature`.
//!
//! The signed bytes are `"<timestamp>." + body`, keyed with the endpoint's
//! secret; the header carries the timestamp and one or more `v1=` hex digests
//! (more than one while a secret is being rotated). A receiver refuses a
//! signature further than [`SIGNATURE_TOLERANCE_SECS`] from its own clock and
//! compares digests in constant time.

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::contract::SIGNATURE_TOLERANCE_SECS;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The hex digest alone, without the `t=`/`v1=` wrapper.
pub fn digest(body: &[u8], secret: &str, timestamp: i64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(body);
    hex(&mac.finalize().into_bytes())
}

/// The whole `Indiefuture-Signature` header value.
pub fn sign(body: &[u8], secret: &str, timestamp: i64) -> String {
    format!("t={timestamp},v1={}", digest(body, secret, timestamp))
}

/// Check a delivery's signature and return its parsed body. `now` is unix
/// seconds. The error is a short reason, safe to log.
pub fn verify(body: &[u8], header: &str, secret: &str, now: i64) -> Result<Value, &'static str> {
    let (mut timestamp, mut supplied) = (None, Vec::new());
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", value)) => timestamp = value.parse::<i64>().ok(),
            Some(("v1", value)) => supplied.push(value),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or("the signature has no timestamp")?;
    if (now - timestamp).abs() > SIGNATURE_TOLERANCE_SECS {
        return Err("the signature is too old or from the future");
    }
    let expected = digest(body, secret, timestamp);
    if !supplied.iter().any(|s| bool::from(s.as_bytes().ct_eq(expected.as_bytes()))) {
        return Err("the signature does not match");
    }
    serde_json::from_slice(body).map_err(|_| "the payload is not JSON")
}

/// Check against several secrets: the one held for the registered endpoint,
/// then any configured fallback. Returns the body on the first that matches.
pub fn verify_any(body: &[u8], header: &str, secrets: &[String], now: i64) -> Result<Value, &'static str> {
    let mut last = "no webhook secret is configured";
    for secret in secrets {
        match verify(body, header, secret, now) {
            Ok(value) => return Ok(value),
            Err(reason) => last = reason,
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "whsec_test";
    const BODY: &[u8] = br#"{"id":"1","type":"episode.ready"}"#;

    #[test]
    fn a_fresh_signature_made_with_the_secret_is_the_only_one_accepted() {
        let header = sign(BODY, SECRET, 1_000);
        assert_eq!(verify(BODY, &header, SECRET, 1_000).unwrap()["type"], "episode.ready");
        // Still inside the five-minute window.
        assert!(verify(BODY, &header, SECRET, 1_000 + SIGNATURE_TOLERANCE_SECS).is_ok());

        assert_eq!(verify(BODY, &header, "whsec_other", 1_000), Err("the signature does not match"));
        assert_eq!(verify(b"{}", &header, SECRET, 1_000), Err("the signature does not match"));
        assert_eq!(verify(BODY, &header, SECRET, 1_000 + SIGNATURE_TOLERANCE_SECS + 1), Err("the signature is too old or from the future"));
        assert_eq!(verify(BODY, &header, SECRET, 1_000 - SIGNATURE_TOLERANCE_SECS - 1), Err("the signature is too old or from the future"));
        assert_eq!(verify(BODY, "v1=deadbeef", SECRET, 1_000), Err("the signature has no timestamp"));
    }

    #[test]
    fn a_rotating_endpoint_may_send_several_digests() {
        let old = digest(BODY, "whsec_old", 1_000);
        let header = format!("t=1000,v1={old},v1={}", digest(BODY, SECRET, 1_000));
        assert!(verify(BODY, &header, SECRET, 1_000).is_ok());
        assert!(verify(BODY, &header, "whsec_old", 1_000).is_ok());
    }

    #[test]
    fn verify_any_tries_every_secret_and_reports_the_last_reason() {
        let header = sign(BODY, SECRET, 1_000);
        let secrets = vec!["whsec_stale".to_owned(), SECRET.to_owned()];
        assert!(verify_any(BODY, &header, &secrets, 1_000).is_ok());
        assert_eq!(verify_any(BODY, &header, &["whsec_stale".to_owned()], 1_000), Err("the signature does not match"));
        assert_eq!(verify_any(BODY, &header, &[], 1_000), Err("no webhook secret is configured"));
    }

    #[test]
    fn a_body_that_is_not_json_is_refused_even_when_signed() {
        let header = sign(b"not json", SECRET, 1_000);
        assert_eq!(verify(b"not json", &header, SECRET, 1_000), Err("the payload is not JSON"));
    }
}
