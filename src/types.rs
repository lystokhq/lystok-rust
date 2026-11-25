//! Shapes of the Lystok API v1, with the field names the API uses.
//! Reference: <https://api.lystok.com/docs>

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A file attached to an email.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// The name the recipient sees.
    pub filename: String,
    /// The file, base64-encoded.
    pub content: String,
    /// MIME type; guessed from the file name when missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl Attachment {
    /// `content` is the file itself; it is base64-encoded here.
    pub fn new(filename: impl Into<String>, content: impl AsRef<[u8]>) -> Self {
        use base64::Engine as _;
        Self {
            filename: filename.into(),
            content: base64::engine::general_purpose::STANDARD.encode(content),
            content_type: None,
        }
    }
}

/// Count opens and clicks. Off by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracking {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opens: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clicks: Option<bool>,
}

/// One email. Give either `html` (and `subject`) or `template_id`.
///
/// ```
/// let email = lystok::SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com")
///     .subject("Your order #1042")
///     .html("<p>Thank you!</p>");
/// ```
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SendEmailParams {
    /// Sender: an address on a verified domain of your account, with an optional name.
    pub from: String,
    /// One recipient address.
    pub to: String,
    /// Required without `template_id`; with a template it replaces the template's subject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Plain-text version. Made from `html` when not given.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A template from your account (Templates → ⋯ → Copy ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    /// Values for the template's `{{ variables }}`: a JSON object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Which language version of the template to use: `uk`, `en`, `pl`, `de`, `es`, `fr`, `ru`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Extra headers: `X-*`, `List-Unsubscribe`, `List-Unsubscribe-Post`, `References`, `In-Reply-To`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    /// Up to 10 key/value pairs to find the email by later.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking: Option<Tracking>,
    /// Up to 10 files, 7 MB together (before base64).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
}

impl SendEmailParams {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            ..Self::default()
        }
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn html(mut self, html: impl Into<String>) -> Self {
        self.html = Some(html.into());
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// A template, with values for its variables (any `Serialize` that makes a JSON object,
    /// such as `serde_json::json!({ "first_name": "Olena" })`).
    pub fn template(mut self, template_id: impl Into<String>, data: impl Serialize) -> Self {
        self.template_id = Some(template_id.into());
        self.data = Some(serde_json::to_value(data).expect("template data must serialize to JSON"));
        self
    }

    pub fn locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    pub fn reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers
            .get_or_insert_with(BTreeMap::new)
            .insert(name.into(), value.into());
        self
    }

    pub fn tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags
            .get_or_insert_with(BTreeMap::new)
            .insert(key.into(), value.into());
        self
    }

    pub fn tracking(mut self, opens: bool, clicks: bool) -> Self {
        self.tracking = Some(Tracking {
            opens: Some(opens),
            clicks: Some(clicks),
        });
        self
    }

    pub fn attachment(mut self, attachment: Attachment) -> Self {
        self.attachments
            .get_or_insert_with(Vec::new)
            .push(attachment);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailCreated {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailBatchCreated {
    /// In the same order as the emails sent.
    pub data: Vec<EmailCreated>,
}

/// queued → sent → delivered; or bounced, complained, failed, suppressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EmailState {
    Queued,
    Sent,
    Delivered,
    Bounced,
    Complained,
    Failed,
    Suppressed,
    /// A state this version of the library does not know yet.
    #[serde(other)]
    Unknown,
}

/// What happened to an email.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Email {
    pub id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    #[serde(default)]
    pub template_id: Option<String>,
    pub state: EmailState,
    /// Why it failed or was suppressed.
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    pub created_at: String,
    #[serde(default)]
    pub sent_at: Option<String>,
    #[serde(default)]
    pub delivered_at: Option<String>,
    #[serde(default)]
    pub bounced_at: Option<String>,
    #[serde(default)]
    pub complained_at: Option<String>,
    #[serde(default)]
    pub opened_at: Option<String>,
    #[serde(default)]
    pub clicked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContactStatus {
    Subscribed,
    Unsubscribed,
    Bounced,
    Complained,
    /// A status this version of the library does not know yet.
    #[serde(other)]
    Unknown,
}

/// A person in your contacts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub locale: Option<String>,
    pub status: ContactStatus,
    /// Your own fields by key: strings, numbers or booleans.
    #[serde(default)]
    pub fields: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub lists: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: String,
    /// How the person agreed to get your emails.
    #[serde(default)]
    pub consent_source: Option<String>,
    #[serde(default)]
    pub consent_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Adds a person, or updates them when the address is already there.
///
/// `locale: Some(None)` clears the language; a `null` in `fields` removes that value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpsertContactParams {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, serde_json::Value>>,
    /// Lists by name; ones that do not exist yet are made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lists: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// How the person agreed to get your emails. Required for a new contact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent_source: Option<String>,
}

impl UpsertContactParams {
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            ..Self::default()
        }
    }
}

/// Changes to a person. `lists` and `tags` are added; `remove_lists` and `remove_tags` are taken
/// away. `status: Some(ContactStatus::Unsubscribed)` unsubscribes the person from your newsletters.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UpdateContactParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lists: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_lists: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ContactStatus>,
}

/// A webhook event. `type` is one of `email.sent`, `email.delivered`, `email.bounced`,
/// `email.complained`, `email.failed`, `email.suppressed`, `email.opened`, `email.clicked`,
/// `contact.unsubscribed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebhookEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub created_at: String,
    pub data: serde_json::Value,
}
