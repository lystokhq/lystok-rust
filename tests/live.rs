//! Against a real Lystok API; does nothing unless LYSTOK_LIVE=1. Needs a full-access key and a
//! sender on a verified domain of that account:
//!
//! ```sh
//! LYSTOK_LIVE=1 LYSTOK_API_KEY=lk_... LYSTOK_BASE_URL=http://localhost:3200 \
//! LYSTOK_TEST_FROM=hello@your-domain LYSTOK_TEST_TO=you@your-domain cargo test --test live
//! ```

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use lystok::{
    ContactStatus, Lystok, RequestOptions, SendEmailParams, UpdateContactParams,
    UpsertContactParams,
};

struct Live {
    lystok: Lystok,
    from: String,
    to: String,
}

impl Live {
    fn email(&self, subject: &str) -> SendEmailParams {
        SendEmailParams::new(&self.from, &self.to)
            .subject(subject)
            .text("lystok-rust live test")
    }
}

fn live() -> Option<Live> {
    if std::env::var("LYSTOK_LIVE").as_deref() != Ok("1") {
        eprintln!("skipped: set LYSTOK_LIVE=1 to run");
        return None;
    }
    let mut builder = Lystok::builder();
    if let Ok(url) = std::env::var("LYSTOK_BASE_URL") {
        builder = builder.base_url(url);
    }
    Some(Live {
        lystok: builder.build().unwrap(),
        from: std::env::var("LYSTOK_TEST_FROM").unwrap(),
        to: std::env::var("LYSTOK_TEST_TO").unwrap(),
    })
}

fn unique() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[tokio::test]
async fn send_and_read_back() {
    let Some(t) = live() else { return };
    let sent = t
        .lystok
        .emails()
        .send(&t.email("lystok-rust live test").tag("sdk", "rust"))
        .await
        .unwrap();
    let email = t.lystok.emails().get(&sent.id).await.unwrap();
    assert_eq!((email.id, email.to), (sent.id, t.to.clone()));
    assert_eq!(
        email.tags,
        BTreeMap::from([("sdk".to_owned(), "rust".to_owned())])
    );
}

#[tokio::test]
async fn once_per_idempotency_key() {
    let Some(t) = live() else { return };
    let options = RequestOptions::default().idempotency_key(format!("lystok-rust-{}", unique()));
    let first = t
        .lystok
        .emails()
        .send_with(&t.email("lystok-rust idempotency"), options.clone())
        .await
        .unwrap();
    let second = t
        .lystok
        .emails()
        .send_with(&t.email("lystok-rust idempotency"), options)
        .await
        .unwrap();
    assert_eq!(first.id, second.id);
}

#[tokio::test]
async fn batch() {
    let Some(t) = live() else { return };
    let res = t
        .lystok
        .emails()
        .send_batch(&[
            t.email("lystok-rust batch 1"),
            t.email("lystok-rust batch 2"),
        ])
        .await
        .unwrap();
    assert_eq!(res.data.len(), 2);
}

#[tokio::test]
async fn validation_error() {
    let Some(t) = live() else { return };
    let email = SendEmailParams::new(&t.from, "not-an-address")
        .subject("s")
        .text("t");
    let err = t.lystok.emails().send(&email).await.unwrap_err();
    assert_eq!((err.status(), err.code()), (400, "validation"));
    assert!(err.request_id().is_some());
}

#[tokio::test]
async fn contact() {
    let Some(t) = live() else { return };
    let address = format!("lystok-rust-{}@example.com", unique());
    let made = t
        .lystok
        .contacts()
        .upsert(&UpsertContactParams {
            first_name: Some("Olena".into()),
            tags: Some(vec!["sdk-test".into()]),
            consent_source: Some("lystok-rust live test".into()),
            ..UpsertContactParams::new(&address)
        })
        .await
        .unwrap();
    assert_eq!(
        (made.status, made.tags.clone()),
        (ContactStatus::Subscribed, vec!["sdk-test".to_owned()])
    );
    let changed = t
        .lystok
        .contacts()
        .update(
            &address,
            &UpdateContactParams {
                last_name: Some("Shevchenko".into()),
                remove_tags: Some(vec!["sdk-test".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (changed.last_name.as_deref(), changed.tags.len()),
        (Some("Shevchenko"), 0)
    );
    assert_eq!(
        t.lystok.contacts().get(&made.id).await.unwrap().email,
        address
    );
    t.lystok.contacts().delete(&made.id).await.unwrap();
    let err = t.lystok.contacts().get(&address).await.unwrap_err();
    assert_eq!(err.status(), 404);
}
