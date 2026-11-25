# Lystok for Rust

The official library for the [Lystok](https://lystok.com) email API: send transactional email,
keep contacts in step and check webhooks. Async, on `reqwest` with rustls (no OpenSSL) and
`tokio`; Rust 1.85+.

```sh
cargo add lystok
```

## Send an email

Create an API key in Lystok → Settings → For developers, and verify your sender domain.

```rust
use lystok::{Lystok, SendEmailParams};

let lystok = Lystok::new(std::env::var("LYSTOK_API_KEY")?);

let email = lystok
    .emails()
    .send(&SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com")
        .subject("Your order #1042")
        .html("<p>Thank you! We are packing your order.</p>"))
    .await?;
println!("{}", email.id);
```

The key is always passed in: keep it wherever you keep secrets, under any name (`LYSTOK_API_KEY`
above is only an example); the library never reads the environment itself. `Lystok::new` panics on
an empty key; `Lystok::builder().api_key(…).build()` returns an error instead. Keys belong on your
server: never put one in a browser or a mobile app. The client is cheap to clone; clones share one connection pool.

### With a template

```rust
use serde_json::json;

lystok
    .emails()
    .send(&SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com")
        .template("019ab3f2-…", json!({ "first_name": "Olena", "order": { "number": 1042 } })) // Templates → ⋯ → Copy ID
        .locale("uk"))
    .await?;
```

Any `Serialize` value that makes a JSON object works as template data, your own structs included.

### Attachments, tags, tracking

```rust
use lystok::Attachment;

lystok
    .emails()
    .send(&SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com")
        .subject("Invoice #1042")
        .html("<p>Your invoice is attached.</p>")
        .attachment(Attachment::new("invoice-1042.pdf", std::fs::read("invoice-1042.pdf")?)) // base64-encoded for you
        .tag("type", "invoice")
        .tracking(true, true))
    .await?;
```

`SendEmailParams` is also a plain struct with public fields, if you prefer
`SendEmailParams { subject: Some(…), ..SendEmailParams::new(from, to) }`.

### Many at once, and what happened

```rust
use lystok::EmailState;

let res = lystok
    .emails()
    .send_batch(&[
        SendEmailParams::new("Shop <orders@shop.example>", "olena@example.com").template("…", json!({ "first_name": "Olena" })),
        SendEmailParams::new("Shop <orders@shop.example>", "taras@example.com").template("…", json!({ "first_name": "Taras" })),
    ])
    .await?; // up to 100; ids in the same order

let email = lystok.emails().get(&res.data[0].id).await?;
if email.state == EmailState::Delivered {
    // …
}
```

## Sending once

Every send carries an `Idempotency-Key`, so the library's own retries never send an email twice.
To be safe across your own retries too (a crashed job, a repeated webhook), pass your own key —
the same key within 24 hours gets the first answer back:

```rust
use lystok::RequestOptions;

lystok
    .emails()
    .send_with(&email, RequestOptions::default().idempotency_key(format!("order-{}", order.id)))
    .await?;
```

## Contacts

Needs a key with full access.

```rust
use lystok::{ContactStatus, UpdateContactParams, UpsertContactParams};

let contact = lystok
    .contacts()
    .upsert(&UpsertContactParams {
        first_name: Some("Olena".into()),
        lists: Some(vec!["Customers".into()]),
        tags: Some(vec!["bought-online".into()]),
        consent_source: Some("Newsletter checkbox at checkout".into()), // required for a new contact
        ..UpsertContactParams::new("olena@example.com")
    })
    .await?;

lystok.contacts().get("olena@example.com").await?; // by address or by id
lystok
    .contacts()
    .update("olena@example.com", &UpdateContactParams {
        remove_tags: Some(vec!["bought-online".into()]),
        fields: Some([("city".to_owned(), json!("Lviv"))].into()),
        ..Default::default()
    })
    .await?;
lystok
    .contacts()
    .update("olena@example.com", &UpdateContactParams { status: Some(ContactStatus::Unsubscribed), ..Default::default() })
    .await?;
lystok.contacts().delete(&contact.id).await?;
```

## Errors

Every failure is a `lystok::Error`:

```rust
match lystok.emails().send(&email).await {
    Ok(sent) => println!("queued {}", sent.id),
    Err(e) => {
        e.status();     // 400, 401, 403, 404, 409, 429, 5xx — or 0 when no answer came
        e.code();       // "validation", "not_found", "plan_limit", "access_ended", "timeout", "connection_error", …
        e.message();    // for people
        e.issues();     // [Issue { path: "to", message: "…" }] for validation errors
        e.request_id(); // quote it when you write to support
    }
}
```

`Error` implements `std::error::Error`, so `?` into `anyhow` or `Box<dyn Error>` works.

## Retries and timeouts

Network errors, timeouts, `429` and `5xx` are tried again twice, waiting a little longer each time
(or as long as `Retry-After` says). Each try may take 30 seconds.

```rust
use std::time::Duration;

let lystok = Lystok::builder()
    .api_key(std::env::var("LYSTOK_API_KEY")?)
    .max_retries(4)
    .timeout(Duration::from_secs(10))
    .build()?;

lystok.emails().get_with(&id, RequestOptions::default().timeout(Duration::from_secs(5))).await?;
```

`.http_client(reqwest::Client)` uses a client of your own, for proxies or a shared pool.

## Webhooks

Lystok signs every webhook ([Standard Webhooks](https://www.standardwebhooks.com)). Check the
**raw body** before parsing it; `lystok::webhooks::verify` returns the event or a
`WebhookVerificationError`. The secret (`whsec_…`) is in the webhook's menu → Signing secret.

With axum:

```rust
use axum::{body::Bytes, http::{HeaderMap, StatusCode}};

async fn lystok_webhook(headers: HeaderMap, body: Bytes) -> StatusCode {
    let secret = std::env::var("LYSTOK_WEBHOOK_SECRET").expect("LYSTOK_WEBHOOK_SECRET");
    let Ok(event) = lystok::webhooks::verify(&body, &headers, &secret) else {
        return StatusCode::UNAUTHORIZED;
    };
    if event.event_type == "email.bounced" {
        // answer within 10 seconds: hand slow work to a queue
    }
    StatusCode::OK
}
```

Headers can be an `http::HeaderMap` (axum, hyper, actix-web via `http`), a `HashMap` or
`BTreeMap` of names to values, or a slice of `(name, value)` pairs; names match in any case.

The same event can arrive twice: skip ones you have handled by the `webhook-id` header.

## Development

```sh
cargo test
LYSTOK_LIVE=1 LYSTOK_API_KEY=lk_… LYSTOK_TEST_FROM=… LYSTOK_TEST_TO=… cargo test --test live   # also against the real API
```

`tests/fixtures/webhooks.json` holds webhook signatures made by the Lystok platform itself; the same
file is checked by every Lystok library.

## License

[MIT](LICENSE)
