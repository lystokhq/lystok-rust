use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, RETRY_AFTER, USER_AGENT};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{self, Error, Issue};
use crate::resources::{Contacts, Emails, Webhooks};

const DEFAULT_BASE_URL: &str = "https://api.lystok.com";
const RETRY_STATUSES: [u16; 6] = [408, 429, 500, 502, 503, 504];
const USER_AGENT_VALUE: &str = concat!("lystok-rust/", env!("CARGO_PKG_VERSION"));

/// The Lystok API client. Cheap to clone; clones share one connection pool.
///
/// ```no_run
/// # async fn run() -> Result<(), lystok::Error> {
/// use lystok::{Lystok, SendEmailParams};
///
/// let lystok = Lystok::from_env()?; // reads LYSTOK_API_KEY
/// let email = lystok
///     .emails()
///     .send(&SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com")
///         .subject("Your order #1042")
///         .html("<p>Thank you!</p>"))
///     .await?;
/// println!("{}", email.id);
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Lystok {
    inner: Arc<Inner>,
}

struct Inner {
    api_key: String,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
    http: reqwest::Client,
}

impl std::fmt::Debug for Lystok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lystok")
            .field("base_url", &self.inner.base_url)
            .field("timeout", &self.inner.timeout)
            .field("max_retries", &self.inner.max_retries)
            .finish_non_exhaustive()
    }
}

/// Per-call options.
#[derive(Debug, Clone, Default)]
pub struct RequestOptions {
    /// For sends: the same key within 24 hours gets the first answer back instead of a second
    /// email. Use your order id, for example. Made up for you when not given, which protects
    /// the call's own retries.
    pub idempotency_key: Option<String>,
    /// How long one attempt of this call may wait, instead of the client's timeout.
    pub timeout: Option<Duration>,
}

impl RequestOptions {
    pub fn idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Builds a [`Lystok`] client with other settings.
#[derive(Debug, Default)]
#[must_use]
pub struct LystokBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    timeout: Option<Duration>,
    max_retries: Option<u32>,
    http: Option<reqwest::Client>,
}

impl LystokBuilder {
    /// An API key from Settings → For developers; `LYSTOK_API_KEY` when not given.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Default `https://api.lystok.com`.
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// How long one attempt may wait for the API. Default 30 seconds.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// How many times to try again after a network error, a timeout, 429 or 5xx. Default 2.
    /// Safe for emails too: every send carries an `Idempotency-Key`.
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Your own `reqwest::Client`, for proxies or shared connection pools.
    pub fn http_client(mut self, client: reqwest::Client) -> Self {
        self.http = Some(client);
        self
    }

    /// Fails with code `missing_api_key` when no key is given and `LYSTOK_API_KEY` is not set.
    pub fn build(self) -> Result<Lystok, Error> {
        let api_key = self
            .api_key
            .or_else(|| std::env::var("LYSTOK_API_KEY").ok())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                Error::new(
                    0,
                    "missing_api_key",
                    "Missing API key: pass it to Lystok::new(\"lk_live_...\") or set LYSTOK_API_KEY",
                )
            })?;
        let http = match self.http {
            Some(http) => http,
            None => reqwest::Client::builder().build().map_err(|e| {
                Error::new(0, "connection_error", format!("Could not set up HTTP: {e}"))
            })?,
        };
        Ok(Lystok {
            inner: Arc::new(Inner {
                api_key,
                base_url: self
                    .base_url
                    .as_deref()
                    .unwrap_or(DEFAULT_BASE_URL)
                    .trim_end_matches('/')
                    .to_owned(),
                timeout: self.timeout.unwrap_or(Duration::from_secs(30)),
                max_retries: self.max_retries.unwrap_or(2),
                http,
            }),
        })
    }
}

impl Lystok {
    /// A client with this API key and the default settings.
    ///
    /// # Panics
    /// When the key is empty, or the system's TLS cannot be set up.
    pub fn new(api_key: impl Into<String>) -> Self {
        let api_key = api_key.into();
        assert!(!api_key.is_empty(), "Missing API key");
        Self::builder()
            .api_key(api_key)
            .build()
            .expect("could not set up the Lystok client")
    }

    /// A client with the key in `LYSTOK_API_KEY`.
    pub fn from_env() -> Result<Self, Error> {
        Self::builder().build()
    }

    pub fn builder() -> LystokBuilder {
        LystokBuilder::default()
    }

    /// Transactional email.
    pub fn emails(&self) -> Emails<'_> {
        Emails { client: self }
    }

    /// Contacts. Needs a key with full access.
    pub fn contacts(&self) -> Contacts<'_> {
        Contacts { client: self }
    }

    /// Webhook signature checks; the same as [`crate::webhooks::verify`].
    pub fn webhooks(&self) -> Webhooks {
        Webhooks
    }

    pub(crate) async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&(impl Serialize + ?Sized)>,
        options: RequestOptions,
    ) -> Result<T, Error> {
        let (status, bytes) = self.send(method, path, body, options).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
            Error::new(
                status,
                "invalid_response",
                format!("Could not read the Lystok API answer: {e}"),
            )
        })
    }

    /// Returns the status and body of a successful answer.
    pub(crate) async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&(impl Serialize + ?Sized)>,
        options: RequestOptions,
    ) -> Result<(u16, Vec<u8>), Error> {
        let inner = &self.inner;
        let url = format!("{}/v1{}", inner.base_url, path);
        let body = body.map(serde_json::to_vec).transpose().map_err(|e| {
            Error::new(
                0,
                "invalid_request",
                format!("Could not encode the request: {e}"),
            )
        })?;
        let wait = options.timeout.unwrap_or(inner.timeout);

        let mut attempt: u32 = 0;
        loop {
            let mut req = inner
                .http
                .request(method.clone(), &url)
                .timeout(wait)
                .header(AUTHORIZATION, format!("Bearer {}", inner.api_key))
                .header(ACCEPT, "application/json")
                .header(USER_AGENT, USER_AGENT_VALUE);
            if let Some(body) = &body {
                req = req
                    .header(CONTENT_TYPE, "application/json")
                    .body(body.clone());
            }
            if let Some(key) = &options.idempotency_key {
                req = req.header("Idempotency-Key", key);
            }

            let error = match req.send().await {
                Ok(res) => {
                    let status = res.status().as_u16();
                    let headers = res.headers().clone();
                    match res.bytes().await {
                        Err(e) => network_error(&e, wait),
                        Ok(bytes) if status < 300 => return Ok((status, bytes.to_vec())),
                        // A DELETE whose first try went through before the network failed finds nothing.
                        Ok(_) if status == 404 && method == Method::DELETE && attempt > 0 => {
                            return Ok((status, Vec::new()));
                        }
                        Ok(bytes) => {
                            let error = error_from(status, &headers, &bytes);
                            let retry = RETRY_STATUSES.contains(&status)
                                || error.code() == "idempotency_in_progress";
                            if !retry {
                                return Err(error);
                            }
                            error
                        }
                    }
                }
                Err(e) => network_error(&e, wait),
            };
            if attempt >= inner.max_retries {
                return Err(error);
            }
            tokio::time::sleep(backoff(attempt, error.retry_after())).await;
            attempt += 1;
        }
    }
}

fn network_error(e: &reqwest::Error, wait: Duration) -> Error {
    if e.is_timeout() {
        Error::new(
            0,
            "timeout",
            format!(
                "No answer from the Lystok API within {} s",
                wait.as_secs_f64()
            ),
        )
    } else {
        Error::new(
            0,
            "connection_error",
            format!("Could not reach the Lystok API: {e}"),
        )
    }
}

fn backoff(attempt: u32, retry_after: Option<Duration>) -> Duration {
    if let Some(after) = retry_after {
        return after.min(Duration::from_secs(60));
    }
    let base = (0.5 * 2f64.powi(attempt.min(16) as i32)).min(8.0);
    Duration::from_secs_f64(base * (0.75 + random_fraction() * 0.5))
}

/// A number in [0, 1) for jitter, without a random-number crate.
fn random_fraction() -> f64 {
    let bits = RandomState::new().build_hasher().finish();
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

fn error_from(status: u16, headers: &HeaderMap, body: &[u8]) -> Error {
    #[derive(serde::Deserialize, Default)]
    struct Body {
        #[serde(default)]
        error: ApiError,
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct ApiError {
        code: Option<String>,
        message: Option<String>,
        params: Option<serde_json::Map<String, serde_json::Value>>,
        issues: Option<Vec<Issue>>,
        request_id: Option<String>,
    }

    let e = serde_json::from_slice::<Body>(body)
        .unwrap_or_default()
        .error;
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let retry_after = header(RETRY_AFTER.as_str())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(Duration::from_secs_f64);
    Error::from_inner(error::Inner {
        status,
        code: e.code.unwrap_or_else(|| {
            if status >= 500 {
                "server_error"
            } else {
                "http_error"
            }
            .to_owned()
        }),
        message: e
            .message
            .unwrap_or_else(|| format!("The Lystok API answered {status}")),
        params: e.params,
        issues: e.issues,
        request_id: e
            .request_id
            .or_else(|| header("x-request-id").map(str::to_owned)),
        retry_after,
    })
}

/// Percent-encodes one path segment, like `encodeURIComponent`.
pub(crate) fn segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments() {
        assert_eq!(
            segment("olena+shop@example.com"),
            "olena%2Bshop%40example.com"
        );
        assert_eq!(segment("019ab3f2-7c1e"), "019ab3f2-7c1e");
        assert_eq!(segment("a/b ї"), "a%2Fb%20%D1%97");
    }

    #[test]
    fn backoff_grows_and_caps() {
        for attempt in 0..10 {
            let d = backoff(attempt, None).as_secs_f64();
            let base = (0.5 * 2f64.powi(attempt as i32)).min(8.0);
            assert!(d >= base * 0.75 && d <= base * 1.25, "{attempt}: {d}");
        }
        assert_eq!(
            backoff(0, Some(Duration::from_secs(600))),
            Duration::from_secs(60)
        );
    }
}
