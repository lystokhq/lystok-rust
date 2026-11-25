use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lystok::webhooks::{VerifyOptions, verify_with};
use lystok::{Lystok, WebhookEvent};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixtures {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    secret: String,
    payload: String,
    headers: HashMap<String, String>,
    now: u64,
    valid: bool,
}

// Signed by the Lystok platform itself, so passing them means accepting exactly what it sends.
fn cases() -> Vec<Case> {
    serde_json::from_str::<Fixtures>(include_str!("fixtures/webhooks.json"))
        .unwrap()
        .cases
}

fn at(case: &Case) -> VerifyOptions {
    VerifyOptions {
        now: Some(UNIX_EPOCH + Duration::from_secs(case.now)),
        ..VerifyOptions::default()
    }
}

fn valid() -> Case {
    cases().into_iter().find(|c| c.name == "valid").unwrap()
}

#[test]
fn platform_cases() {
    let cases = cases();
    assert_eq!(cases.len(), 15);
    for case in &cases {
        let result = verify_with(
            case.payload.as_bytes(),
            &case.headers,
            &case.secret,
            at(case),
        );
        if case.valid {
            let event = result.unwrap_or_else(|e| panic!("{}: {e}", case.name));
            let expected: serde_json::Value = serde_json::from_str(&case.payload).unwrap();
            assert_eq!(
                serde_json::to_value(&event).unwrap(),
                expected,
                "{}",
                case.name
            );
        } else {
            assert!(result.is_err(), "{} should fail", case.name);
        }
    }
}

#[test]
fn header_sources() {
    let case = valid();
    let check =
        |event: Result<WebhookEvent, _>| assert_eq!(event.unwrap().event_type, "email.delivered");

    let mut map = http::HeaderMap::new();
    for (k, v) in &case.headers {
        map.insert(
            http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
            v.parse().unwrap(),
        );
    }
    check(verify_with(
        case.payload.as_bytes(),
        &map,
        &case.secret,
        at(&case),
    ));

    let upper: BTreeMap<String, &str> = case
        .headers
        .iter()
        .map(|(k, v)| (k.to_uppercase(), v.as_str()))
        .collect();
    check(verify_with(
        case.payload.as_bytes(),
        &upper,
        &case.secret,
        at(&case),
    ));

    let pairs: Vec<(&str, &str)> = case
        .headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    check(verify_with(
        case.payload.as_bytes(),
        pairs.as_slice(),
        &case.secret,
        at(&case),
    ));
}

#[test]
fn signature_values_split_over_header_lines() {
    let case = valid();
    let mut map = http::HeaderMap::new();
    for (k, v) in &case.headers {
        let name = http::HeaderName::from_bytes(k.as_bytes()).unwrap();
        if k == "webhook-signature" {
            map.append(name.clone(), "v1,AAAA".parse().unwrap());
        }
        map.append(name, v.parse().unwrap());
    }
    assert!(verify_with(case.payload.as_bytes(), &map, &case.secret, at(&case)).is_ok());
}

#[test]
fn secret_that_is_not_base64() {
    let case = valid();
    assert!(
        verify_with(
            case.payload.as_bytes(),
            &case.headers,
            "whsec_not base64!",
            at(&case)
        )
        .is_err()
    );
}

#[test]
fn signature_with_other_characters() {
    let case = valid();
    let mut headers = case.headers.clone();
    headers.insert("webhook-signature".into(), "v1,ïñvalid".into());
    assert!(verify_with(case.payload.as_bytes(), &headers, &case.secret, at(&case)).is_err());
}

#[test]
fn checks_the_clock_by_default() {
    let case = valid();
    // The fixtures were signed long before (or after) today, so the real clock refuses them.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(now.abs_diff(case.now) > 300);
    let err =
        lystok::webhooks::verify(case.payload.as_bytes(), &case.headers, &case.secret).unwrap_err();
    assert_eq!(err.to_string(), "Webhook timestamp is too far from now");
}

#[test]
fn on_the_client() {
    let case = valid();
    let event = Lystok::new("lk_test_x")
        .webhooks()
        .verify_with(
            case.payload.as_bytes(),
            &case.headers,
            &case.secret,
            at(&case),
        )
        .unwrap();
    assert_eq!(event.event_type, "email.delivered");
}
