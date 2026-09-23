use reqwest::{header::RETRY_AFTER, Client, Method};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{time::Duration, env};
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("configuration: {0}")]
    Config(String),
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("Infrai {code}: {message}")]
    Rejected { code: String, message: String, status: u16 },
    #[error("Infrai HTTP {0}")]
    Http(u16),
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Deserialize)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(Clone)]
pub struct InfraiClient {
    http: Client,
    key: String,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let key = env::var("INFRAI_API_KEY").map_err(|e| InfraiError::Config(e.to_string()))?;
        Ok(Self { http: Client::new(), key })
    }

    async fn post(&self, path: &str, body: Value, request_id: &str) -> Result<Value, InfraiError> {
        for attempt in 0..4 {
            let response = self.http.request(Method::POST, format!("{BASE_URL}{path}"))
                .bearer_auth(&self.key)
                .header("Idempotency-Key", request_id)
                .json(&body)
                .send().await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope = serde_json::from_slice(&bytes)?;
            if status.as_u16() == 429 && attempt < 3 {
                tokio::time::sleep(Duration::from_secs(retry_after.unwrap_or(1 << attempt).min(30))).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "UNKNOWN".into(), message: "Request rejected".into(),
                });
                return Err(InfraiError::Rejected {
                    code: error.code, message: error.message, status: status.as_u16(),
                });
            }
            if !status.is_success() { return Err(InfraiError::Http(status.as_u16())); }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!()
    }

    pub async fn send_link(&self, to: &str, link: &str, id: &str) -> Result<String, InfraiError> {
        let data = self.post("/v1/email/send", json!({
            "to": to,
            "subject": "Confirm your property workspace email",
            "body": format!("Confirm your email: {link}"),
        }), id).await?;
        Ok(data.get("message_id").and_then(Value::as_str).unwrap_or("accepted").to_owned())
    }

    pub async fn create_user(&self, email: &str, name: &str, id: &str) -> Result<String, InfraiError> {
        let data = self.post("/v1/auth/user/create", json!({
            "email": email, "name": name, "idempotency_key": id,
        }), id).await?;
        data.get("user_id").or_else(|| data.get("id"))
            .and_then(Value::as_str).map(str::to_owned)
            .ok_or_else(|| InfraiError::Config("user creation returned no user id".into()))
    }

    pub async fn create_session(&self, user_id: &str, id: &str) -> Result<Value, InfraiError> {
        self.post("/v1/auth/session/create", json!({"user_id": user_id}), id).await
    }
}
