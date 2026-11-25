mod common;

use std::time::{Duration, Instant};

use common::{Answer, FakeApi, api_error};
use lystok::{
    ContactStatus, Lystok, RequestOptions, SendEmailParams, UpdateContactParams,
    UpsertContactParams,
};
use serde_json::json;

fn simple_email() -> SendEmailParams {
    SendEmailParams::new("a@x.example", "b@x.example").text("t")
}

#[tokio::test]
async fn send() {
    let api = FakeApi::start(vec![Answer::json(200, json!({ "id": "019ab3f2" }))]).await;
    let lystok = api.client().build().unwrap();
    let email = SendEmailParams::new("Пекарня <orders@shop.example>", "olena@example.com")
        .subject("Замовлення")
        .html("<p>Hi</p>")
        .tag("type", "order");
    assert_eq!(lystok.emails().send(&email).await.unwrap().id, "019ab3f2");

    let call = &api.calls()[0];
    assert_eq!(
        (call.method.as_str(), call.path.as_str()),
        ("POST", "/v1/emails")
    );
    assert_eq!(
        call.json,
        Some(json!({
            "from": "Пекарня <orders@shop.example>",
            "to": "olena@example.com",
            "subject": "Замовлення",
            "html": "<p>Hi</p>",
            "tags": { "type": "order" },
        }))
    );
    assert_eq!(call.headers["authorization"], "Bearer lk_live_test");
    assert_eq!(call.headers["content-type"], "application/json");
    assert_eq!(call.headers["user-agent"], "lystok-rust/0.1.0");
    let key = &call.headers["idempotency-key"];
    assert_eq!(key.len(), 36, "a UUID: {key}");
    assert_eq!(&key[14..15], "4", "a UUID v4: {key}");
}

#[tokio::test]
async fn batch_with_own_idempotency_key() {
    let api = FakeApi::start(vec![Answer::json(
        200,
        json!({ "data": [{ "id": "a" }, { "id": "b" }] }),
    )])
    .await;
    let lystok = api.client().build().unwrap();
    let emails = [SendEmailParams::new("a@x.example", "b@x.example")
        .template("t", json!({ "first_name": "Olena" }))];
    let res = lystok
        .emails()
        .send_batch_with(
            &emails,
            RequestOptions::default().idempotency_key("order-1042"),
        )
        .await
        .unwrap();
    assert_eq!(
        res.data.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    let call = &api.calls()[0];
    assert_eq!(call.path, "/v1/emails/batch");
    assert_eq!(call.headers["idempotency-key"], "order-1042");
    assert_eq!(
        call.json,
        Some(
            json!([{ "from": "a@x.example", "to": "b@x.example", "template_id": "t", "data": { "first_name": "Olena" } }])
        )
    );
}

#[tokio::test]
async fn get_email() {
    let api = FakeApi::start(vec![Answer::json(200, json!({
        "id": "e1", "from": "a@x.example", "to": "b@x.example", "subject": "s", "template_id": null,
        "state": "delivered", "error": null, "tags": { "sdk": "rust" }, "created_at": "2026-11-24T10:15:02.120Z",
        "sent_at": "2026-11-24T10:15:03.000Z", "delivered_at": "2026-11-24T10:15:04.000Z", "bounced_at": null,
        "complained_at": null, "opened_at": null, "clicked_at": null, "something_new": 1,
    }))])
    .await;
    let email = api
        .client()
        .build()
        .unwrap()
        .emails()
        .get("e1")
        .await
        .unwrap();
    assert_eq!(email.state, lystok::EmailState::Delivered);
    assert_eq!(email.tags["sdk"], "rust");
    assert_eq!(api.calls()[0].path, "/v1/emails/e1");
    assert!(!api.calls()[0].headers.contains_key("idempotency-key"));
}

#[tokio::test]
async fn contacts() {
    let contact = json!({
        "id": "c1", "email": "olena@example.com", "first_name": null, "last_name": null, "phone": null,
        "locale": null, "status": "subscribed", "fields": {}, "lists": [], "tags": [], "source": "api",
        "consent_source": "Checkout", "consent_at": "2026-11-24T10:15:02.120Z",
        "created_at": "2026-11-24T10:15:02.120Z", "updated_at": "2026-11-24T10:15:02.120Z",
    });
    let api = FakeApi::start(vec![
        Answer::json(201, contact.clone()),
        Answer::json(200, contact.clone()),
        Answer::json(200, contact),
        Answer::empty(204),
    ])
    .await;
    let lystok = api.client().build().unwrap();
    let upserted = lystok
        .contacts()
        .upsert(&UpsertContactParams {
            consent_source: Some("Checkout".into()),
            locale: Some(None),
            ..UpsertContactParams::new("olena@example.com")
        })
        .await
        .unwrap();
    assert_eq!(
        (upserted.id.as_str(), upserted.status),
        ("c1", ContactStatus::Subscribed)
    );
    lystok
        .contacts()
        .get("olena+shop@example.com")
        .await
        .unwrap();
    lystok
        .contacts()
        .update(
            "c1",
            &UpdateContactParams {
                remove_tags: Some(vec!["old".into()]),
                status: Some(ContactStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    lystok.contacts().delete("c1").await.unwrap();

    let calls = api.calls();
    let lines: Vec<String> = calls
        .iter()
        .map(|c| format!("{} {}", c.method, c.path))
        .collect();
    assert_eq!(
        lines,
        [
            "POST /v1/contacts",
            "GET /v1/contacts/olena%2Bshop%40example.com",
            "PATCH /v1/contacts/c1",
            "DELETE /v1/contacts/c1",
        ]
    );
    assert_eq!(
        calls[0].json,
        Some(json!({ "email": "olena@example.com", "locale": null, "consent_source": "Checkout" }))
    );
    assert_eq!(
        calls[2].json,
        Some(json!({ "remove_tags": ["old"], "status": "unsubscribed" }))
    );
    assert!(!calls[0].headers.contains_key("idempotency-key"));
}

#[tokio::test]
async fn error_answer() {
    let api = FakeApi::start(vec![Answer::json(
        400,
        json!({ "error": {
            "code": "validation",
            "message": "The request is not valid. See `issues` for the fields.",
            "issues": [{ "path": "to", "message": "Invalid email" }],
            "requestId": "req_1",
        }}),
    )])
    .await;
    let err = api
        .client()
        .build()
        .unwrap()
        .emails()
        .send(&simple_email())
        .await
        .unwrap_err();
    assert_eq!(
        (err.status(), err.code(), err.request_id()),
        (400, "validation", Some("req_1"))
    );
    let issues = err.issues();
    assert_eq!(
        (issues[0].path.as_str(), issues[0].message.as_str()),
        ("to", "Invalid email")
    );
    assert!(err.to_string().contains("not valid"), "{err}");
    assert_eq!(api.calls().len(), 1, "a refused request is not tried again");
}

#[tokio::test]
async fn answer_that_is_not_json() {
    let api = FakeApi::start(vec![
        Answer {
            status: 502,
            body: Some("<html>Bad gateway</html>".into()),
            ..Default::default()
        }
        .header("X-Request-Id", "req_9"),
    ])
    .await;
    let err = api
        .client()
        .max_retries(0)
        .build()
        .unwrap()
        .emails()
        .get("e1")
        .await
        .unwrap_err();
    assert_eq!(
        (err.status(), err.code(), err.request_id()),
        (502, "server_error", Some("req_9"))
    );
}

#[tokio::test]
async fn retries_503_with_the_same_key() {
    let api = FakeApi::start(vec![
        Answer::json(503, api_error("unavailable")),
        Answer::json(200, json!({ "id": "e1" })),
    ])
    .await;
    let res = api
        .client()
        .build()
        .unwrap()
        .emails()
        .send(&simple_email())
        .await
        .unwrap();
    assert_eq!(res.id, "e1");
    let calls = api.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].headers["idempotency-key"],
        calls[1].headers["idempotency-key"]
    );
}

#[tokio::test]
async fn waits_as_long_as_retry_after_says() {
    let api = FakeApi::start(vec![
        Answer::json(429, api_error("rate_limited")).header("Retry-After", "1"),
        Answer::json(200, json!({ "id": "e1" })),
    ])
    .await;
    let started = Instant::now();
    let res = api
        .client()
        .build()
        .unwrap()
        .emails()
        .send(&simple_email())
        .await
        .unwrap();
    assert_eq!(res.id, "e1");
    assert!(
        started.elapsed() >= Duration::from_millis(950),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn retries_idempotency_in_progress_but_not_mismatch() {
    let api = FakeApi::start(vec![
        Answer::json(409, api_error("idempotency_in_progress")),
        Answer::json(200, json!({ "id": "e1" })),
        Answer::json(409, api_error("idempotency_mismatch")),
    ])
    .await;
    let lystok = api.client().build().unwrap();
    assert_eq!(
        lystok.emails().send(&simple_email()).await.unwrap().id,
        "e1"
    );
    let err = lystok.emails().send(&simple_email()).await.unwrap_err();
    assert_eq!(err.code(), "idempotency_mismatch");
    assert_eq!(api.calls().len(), 3);
}

#[tokio::test]
async fn timeout() {
    let api = FakeApi::start(vec![
        Answer::json(200, json!({ "id": "e1" })).delay(Duration::from_millis(800)),
    ])
    .await;
    let lystok = api
        .client()
        .timeout(Duration::from_millis(100))
        .max_retries(0)
        .build()
        .unwrap();
    let err = lystok.emails().get("e1").await.unwrap_err();
    assert_eq!((err.status(), err.code()), (0, "timeout"));
    assert!(err.is_network());
}

#[tokio::test]
async fn per_call_timeout() {
    let api = FakeApi::start(vec![
        Answer::json(200, json!({ "id": "e1" })).delay(Duration::from_millis(800)),
    ])
    .await;
    let lystok = api.client().max_retries(0).build().unwrap();
    let err = lystok
        .emails()
        .send_with(
            &simple_email(),
            RequestOptions::default().timeout(Duration::from_millis(100)),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), "timeout");
}

#[tokio::test]
async fn connection_error() {
    // A port that was just free.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let lystok = Lystok::builder()
        .api_key("lk_live_test")
        .base_url(format!("http://127.0.0.1:{port}"))
        .max_retries(1)
        .build()
        .unwrap();
    let err = lystok.emails().get("e1").await.unwrap_err();
    assert_eq!((err.status(), err.code()), (0, "connection_error"));
}

#[tokio::test]
async fn delete_that_went_through_before_a_failure() {
    let api = FakeApi::start(vec![
        Answer::json(503, api_error("unavailable")),
        Answer::json(404, api_error("not_found")),
    ])
    .await;
    api.client()
        .build()
        .unwrap()
        .contacts()
        .delete("c1")
        .await
        .unwrap();
    assert_eq!(api.calls().len(), 2);
}

#[tokio::test]
async fn delete_of_someone_not_there() {
    let api = FakeApi::start(vec![Answer::json(404, api_error("not_found"))]).await;
    let err = api
        .client()
        .build()
        .unwrap()
        .contacts()
        .delete("c1")
        .await
        .unwrap_err();
    assert_eq!(err.status(), 404);
}

#[test]
fn refuses_an_empty_api_key() {
    for key in ["", "   "] {
        let err = Lystok::builder().api_key(key).build().unwrap_err();
        assert_eq!(err.code(), "missing_api_key");
    }
    // Without .api_key() there is no key at all: the environment is never read.
    assert_eq!(
        Lystok::builder().build().unwrap_err().code(),
        "missing_api_key"
    );
    assert!(std::panic::catch_unwind(|| Lystok::new("")).is_err());
}
