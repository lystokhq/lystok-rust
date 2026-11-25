//! The official library for the [Lystok](https://lystok.com) email API: send transactional
//! email, keep contacts in step and check webhooks.
//!
//! ```no_run
//! # async fn run() -> Result<(), lystok::Error> {
//! use lystok::{Lystok, SendEmailParams};
//!
//! let lystok = Lystok::new(std::env::var("LYSTOK_API_KEY").expect("LYSTOK_API_KEY"));
//! let email = lystok
//!     .emails()
//!     .send(&SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com")
//!         .subject("Your order #1042")
//!         .html("<p>Thank you! We are packing your order.</p>"))
//!     .await?;
//! # Ok(()) }
//! ```
//!
//! Calls are tried again after network errors, timeouts, 429 and 5xx; every email send carries
//! an `Idempotency-Key`, so a retry never sends an email twice. Every failure is an [`Error`].

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod client;
mod error;
mod resources;
pub mod types;
pub mod webhooks;

pub use client::{Lystok, LystokBuilder, RequestOptions};
pub use error::{Error, Issue, WebhookVerificationError};
pub use resources::{Contacts, Emails, Webhooks};
pub use types::{
    Attachment, Contact, ContactStatus, Email, EmailBatchCreated, EmailCreated, EmailState,
    SendEmailParams, Tracking, UpdateContactParams, UpsertContactParams, WebhookEvent,
};
pub use webhooks::{HeaderSource, VerifyOptions};
