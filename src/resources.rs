use reqwest::Method;

use crate::client::{Lystok, RequestOptions, segment};
use crate::error::{Error, WebhookVerificationError};
use crate::types::{
    Contact, Email, EmailBatchCreated, EmailCreated, SendEmailParams, UpdateContactParams,
    UpsertContactParams, WebhookEvent,
};
use crate::webhooks::{self, HeaderSource, VerifyOptions};

/// Transactional email: `lystok.emails()`.
#[derive(Debug, Clone, Copy)]
pub struct Emails<'a> {
    pub(crate) client: &'a Lystok,
}

impl Emails<'_> {
    /// Sends one email. It is queued and usually sent within a second.
    pub async fn send(&self, email: &SendEmailParams) -> Result<EmailCreated, Error> {
        self.send_with(email, RequestOptions::default()).await
    }

    /// [`send`](Self::send) with your own idempotency key or timeout.
    pub async fn send_with(
        &self,
        email: &SendEmailParams,
        options: RequestOptions,
    ) -> Result<EmailCreated, Error> {
        self.client
            .request(Method::POST, "/emails", Some(email), with_key(options))
            .await
    }

    /// Sends up to 100 emails in one request; all are accepted or none. Ids come in the same order.
    pub async fn send_batch(&self, emails: &[SendEmailParams]) -> Result<EmailBatchCreated, Error> {
        self.send_batch_with(emails, RequestOptions::default())
            .await
    }

    /// [`send_batch`](Self::send_batch) with your own idempotency key or timeout.
    pub async fn send_batch_with(
        &self,
        emails: &[SendEmailParams],
        options: RequestOptions,
    ) -> Result<EmailBatchCreated, Error> {
        self.client
            .request(
                Method::POST,
                "/emails/batch",
                Some(emails),
                with_key(options),
            )
            .await
    }

    /// What happened to an email.
    pub async fn get(&self, id: &str) -> Result<Email, Error> {
        self.get_with(id, RequestOptions::default()).await
    }

    /// [`get`](Self::get) with another timeout.
    pub async fn get_with(&self, id: &str, options: RequestOptions) -> Result<Email, Error> {
        self.client
            .request(
                Method::GET,
                &format!("/emails/{}", segment(id)),
                NO_BODY,
                options,
            )
            .await
    }
}

/// Contacts: `lystok.contacts()`. Needs a key with full access.
#[derive(Debug, Clone, Copy)]
pub struct Contacts<'a> {
    pub(crate) client: &'a Lystok,
}

impl Contacts<'_> {
    /// Adds a person, or updates them when the address is already there. Safe to repeat.
    /// `consent_source` is required for a new contact.
    pub async fn upsert(&self, contact: &UpsertContactParams) -> Result<Contact, Error> {
        self.client
            .request(
                Method::POST,
                "/contacts",
                Some(contact),
                RequestOptions::default(),
            )
            .await
    }

    /// By id or by email address.
    pub async fn get(&self, id_or_email: &str) -> Result<Contact, Error> {
        let path = format!("/contacts/{}", segment(id_or_email));
        self.client
            .request(Method::GET, &path, NO_BODY, RequestOptions::default())
            .await
    }

    /// Changes a person, found by id or by email address.
    pub async fn update(
        &self,
        id_or_email: &str,
        changes: &UpdateContactParams,
    ) -> Result<Contact, Error> {
        let path = format!("/contacts/{}", segment(id_or_email));
        self.client
            .request(
                Method::PATCH,
                &path,
                Some(changes),
                RequestOptions::default(),
            )
            .await
    }

    /// Deletes a person, found by id or by email address.
    pub async fn delete(&self, id_or_email: &str) -> Result<(), Error> {
        let path = format!("/contacts/{}", segment(id_or_email));
        self.client
            .send(Method::DELETE, &path, NO_BODY, RequestOptions::default())
            .await
            .map(|_| ())
    }
}

/// Webhook signature checks: `lystok.webhooks()`.
#[derive(Debug, Clone, Copy)]
pub struct Webhooks;

impl Webhooks {
    /// See [`webhooks::verify`].
    pub fn verify(
        &self,
        payload: &[u8],
        headers: &(impl HeaderSource + ?Sized),
        secret: &str,
    ) -> Result<WebhookEvent, WebhookVerificationError> {
        webhooks::verify(payload, headers, secret)
    }

    /// See [`webhooks::verify_with`].
    pub fn verify_with(
        &self,
        payload: &[u8],
        headers: &(impl HeaderSource + ?Sized),
        secret: &str,
        options: VerifyOptions,
    ) -> Result<WebhookEvent, WebhookVerificationError> {
        webhooks::verify_with(payload, headers, secret, options)
    }
}

const NO_BODY: Option<&()> = None;

fn with_key(mut options: RequestOptions) -> RequestOptions {
    if options.idempotency_key.is_none() {
        options.idempotency_key = Some(uuid::Uuid::new_v4().to_string());
    }
    options
}
