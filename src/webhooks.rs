//! Checking that a webhook came from Lystok (the [Standard Webhooks](https://www.standardwebhooks.com)
//! scheme): the headers `webhook-id`, `webhook-timestamp` and `webhook-signature` sign
//! `id.timestamp.body` with HMAC-SHA256.

use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::WebhookVerificationError;
use crate::types::WebhookEvent;

const PREFIX: &str = "whsec_";

/// Where the webhook's headers come from: an `http::HeaderMap` (axum, actix-web via `http`,
/// hyper), a `HashMap` or `BTreeMap` of names to values, or a slice of `(name, value)` pairs.
/// Names are matched in any case.
pub trait HeaderSource {
    /// All values of the header, joined with a space.
    fn header(&self, name: &str) -> Option<String>;
}

impl HeaderSource for http::HeaderMap {
    fn header(&self, name: &str) -> Option<String> {
        let values: Vec<&str> = self
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        (!values.is_empty()).then(|| values.join(" "))
    }
}

fn find<'a, K, V>(pairs: impl IntoIterator<Item = (&'a K, &'a V)>, name: &str) -> Option<String>
where
    K: AsRef<str> + ?Sized + 'a,
    V: AsRef<str> + ?Sized + 'a,
{
    pairs
        .into_iter()
        .find(|(k, _)| k.as_ref().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_ref().to_owned())
}

impl<K: AsRef<str>, V: AsRef<str>, S: BuildHasher> HeaderSource for HashMap<K, V, S> {
    fn header(&self, name: &str) -> Option<String> {
        find(self.iter(), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> HeaderSource for BTreeMap<K, V> {
    fn header(&self, name: &str) -> Option<String> {
        find(self.iter(), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> HeaderSource for [(K, V)] {
    fn header(&self, name: &str) -> Option<String> {
        find(self.iter().map(|(k, v)| (k, v)), name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>, const N: usize> HeaderSource for [(K, V); N] {
    fn header(&self, name: &str) -> Option<String> {
        self.as_slice().header(name)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> HeaderSource for Vec<(K, V)> {
    fn header(&self, name: &str) -> Option<String> {
        self.as_slice().header(name)
    }
}

impl<T: HeaderSource + ?Sized> HeaderSource for &T {
    fn header(&self, name: &str) -> Option<String> {
        (**self).header(name)
    }
}

/// Options of [`verify_with`].
#[derive(Debug, Clone, Copy)]
pub struct VerifyOptions {
    /// How far the timestamp may be from now. Default 5 minutes.
    pub tolerance: Duration,
    /// The current time; for tests. Default: the clock.
    pub now: Option<SystemTime>,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            tolerance: Duration::from_secs(300),
            now: None,
        }
    }
}

/// Checks a webhook and returns its event.
///
/// Pass the **raw body**, exactly as received, before any JSON parsing (`Bytes` in axum).
/// `secret` is the webhook's signing secret (`whsec_...`, from its menu → Signing secret).
pub fn verify(
    payload: &[u8],
    headers: &(impl HeaderSource + ?Sized),
    secret: &str,
) -> Result<WebhookEvent, WebhookVerificationError> {
    verify_with(payload, headers, secret, VerifyOptions::default())
}

/// [`verify`] with another tolerance or clock.
pub fn verify_with(
    payload: &[u8],
    headers: &(impl HeaderSource + ?Sized),
    secret: &str,
    options: VerifyOptions,
) -> Result<WebhookEvent, WebhookVerificationError> {
    let get = |name: &str| headers.header(name).filter(|v| !v.is_empty());
    let (Some(id), Some(timestamp), Some(signatures)) = (
        get("webhook-id"),
        get("webhook-timestamp"),
        get("webhook-signature"),
    ) else {
        return Err(WebhookVerificationError::new(
            "Missing webhook-id, webhook-timestamp or webhook-signature header",
        ));
    };
    if !timestamp.bytes().all(|b| b.is_ascii_digit()) {
        return Err(WebhookVerificationError::new(
            "Invalid webhook-timestamp header",
        ));
    }
    let sent: u64 = timestamp
        .parse()
        .map_err(|_| WebhookVerificationError::new("Invalid webhook-timestamp header"))?;
    let now = options
        .now
        .unwrap_or_else(SystemTime::now)
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if now.abs_diff(sent) > options.tolerance.as_secs() {
        return Err(WebhookVerificationError::new(
            "Webhook timestamp is too far from now",
        ));
    }

    let key = STANDARD
        .decode(secret.strip_prefix(PREFIX).unwrap_or(secret))
        .ok()
        .filter(|k| !k.is_empty())
        .ok_or(WebhookVerificationError::new("Invalid webhook secret"))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).expect("HMAC takes a key of any length");
    mac.update(id.as_bytes());
    mac.update(b".");
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(payload);

    for entry in signatures.split(' ') {
        let Some(("v1", value)) = entry.split_once(',') else {
            continue;
        };
        let Ok(signature) = STANDARD.decode(value) else {
            continue;
        };
        // verify_slice compares in constant time.
        if !signature.is_empty() && mac.clone().verify_slice(&signature).is_ok() {
            return serde_json::from_slice(payload).map_err(|_| {
                WebhookVerificationError::new("The webhook body is not a Lystok event")
            });
        }
    }
    Err(WebhookVerificationError::new(
        "No matching webhook signature",
    ))
}
