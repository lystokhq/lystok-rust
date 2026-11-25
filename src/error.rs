use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A field problem of a `validation` error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// The field, such as `to` or `attachments.0.filename`.
    pub path: String,
    pub message: String,
}

/// Every failure of a Lystok API call.
///
/// [`status`](Self::status) is the HTTP status, or 0 when no answer came ([`code`](Self::code) is
/// then `timeout` or `connection_error`).
///
/// ```no_run
/// # async fn run(lystok: lystok::Lystok, email: lystok::SendEmailParams) {
/// match lystok.emails().send(&email).await {
///     Ok(sent) => println!("queued {}", sent.id),
///     Err(e) if e.code() == "validation" => eprintln!("fix the request: {:?}", e.issues()),
///     Err(e) => eprintln!("{e}"), // has the request id to quote to support
/// }
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Error(Box<Inner>);

#[derive(Debug, Clone)]
pub(crate) struct Inner {
    pub(crate) status: u16,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) params: Option<serde_json::Map<String, serde_json::Value>>,
    pub(crate) issues: Option<Vec<Issue>>,
    pub(crate) request_id: Option<String>,
    pub(crate) retry_after: Option<Duration>,
}

impl Error {
    pub(crate) fn new(status: u16, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::from_inner(Inner {
            status,
            code: code.into(),
            message: message.into(),
            params: None,
            issues: None,
            request_id: None,
            retry_after: None,
        })
    }

    pub(crate) fn from_inner(inner: Inner) -> Self {
        Self(Box::new(inner))
    }

    /// The HTTP status: 400, 401, 403, 404, 409, 429, 5xx — or 0 when no answer came.
    pub fn status(&self) -> u16 {
        self.0.status
    }

    /// `validation`, `not_found`, `plan_limit`, `access_ended`, `rate_limited`, `timeout`,
    /// `connection_error`, …
    pub fn code(&self) -> &str {
        &self.0.code
    }

    /// For people.
    pub fn message(&self) -> &str {
        &self.0.message
    }

    /// Values behind the message, such as a limit.
    pub fn params(&self) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.0.params.as_ref()
    }

    /// Field problems of a validation error.
    pub fn issues(&self) -> &[Issue] {
        self.0.issues.as_deref().unwrap_or_default()
    }

    /// Quote it when you write to support.
    pub fn request_id(&self) -> Option<&str> {
        self.0.request_id.as_deref()
    }

    /// How long the API asked to wait before trying again.
    pub fn retry_after(&self) -> Option<Duration> {
        self.0.retry_after
    }

    /// True when no answer came from the API (a timeout or a network error).
    pub fn is_network(&self) -> bool {
        self.0.status == 0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let e = &self.0;
        if e.status == 0 {
            write!(f, "{} ({})", e.message, e.code)?;
        } else {
            write!(f, "{} ({} {})", e.message, e.status, e.code)?;
        }
        if let Some(id) = &e.request_id {
            write!(f, " [request {id}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {}

/// A webhook whose signature or timestamp is wrong: answer it with 401 and do not trust it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookVerificationError {
    message: &'static str,
}

impl WebhookVerificationError {
    pub(crate) fn new(message: &'static str) -> Self {
        Self { message }
    }
}

impl fmt::Display for WebhookVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for WebhookVerificationError {}
