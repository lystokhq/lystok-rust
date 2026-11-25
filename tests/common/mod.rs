//! A small local HTTP server that answers from a script and records what came.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub path: String,
    /// Lowercase names.
    pub headers: HashMap<String, String>,
    pub json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct Answer {
    pub status: u16,
    pub body: Option<String>,
    pub headers: Vec<(String, String)>,
    pub delay: Option<Duration>,
}

impl Answer {
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Self {
            status,
            body: Some(body.to_string()),
            ..Self::default()
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            ..Self::default()
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

pub fn api_error(code: &str) -> serde_json::Value {
    serde_json::json!({ "error": { "code": code, "message": "x", "requestId": "req_1" } })
}

pub struct FakeApi {
    pub url: String,
    answers: Arc<Mutex<VecDeque<Answer>>>,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl FakeApi {
    pub async fn start(answers: Vec<Answer>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let answers = Arc::new(Mutex::new(VecDeque::from(answers)));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (a, c) = (answers.clone(), calls.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let (a, c) = (a.clone(), c.clone());
                tokio::spawn(async move {
                    let Some(call) = read_request(&mut socket).await else {
                        return;
                    };
                    c.lock().unwrap().push(call);
                    let answer = a
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or_else(|| Answer::json(200, serde_json::json!({})));
                    if let Some(delay) = answer.delay {
                        tokio::time::sleep(delay).await;
                    }
                    let body = answer.body.unwrap_or_default();
                    let mut head = format!(
                        "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                        answer.status,
                        body.len()
                    );
                    if !body.is_empty() {
                        head.push_str("Content-Type: application/json\r\n");
                    }
                    for (k, v) in &answer.headers {
                        head.push_str(&format!("{k}: {v}\r\n"));
                    }
                    head.push_str("\r\n");
                    let _ = socket.write_all(head.as_bytes()).await;
                    let _ = socket.write_all(body.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self {
            url,
            answers,
            calls,
        }
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    #[allow(dead_code)]
    pub fn pending(&self) -> usize {
        self.answers.lock().unwrap().len()
    }

    pub fn client(&self) -> lystok::LystokBuilder {
        lystok::Lystok::builder()
            .api_key("lk_live_test")
            .base_url(&self.url)
    }
}

async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<Call> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = socket.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = &buf[head_end..(head_end + length).min(buf.len())];
    let json = (!body.is_empty()).then(|| serde_json::from_slice(body).unwrap());
    Some(Call {
        method,
        path,
        headers,
        json,
    })
}
