mod infrai_client;

use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, routing::{get, post}, Json, Router};
use infrai_client::{InfraiClient, InfraiError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
struct MaintenanceRequest { property_id: String, summary: String }
#[derive(Clone, Serialize, Deserialize)]
struct TenantDocument { property_id: String, title: String }
#[derive(Clone, Serialize, Deserialize)]
struct InspectionReminder { property_id: String, due_date: String }

#[derive(Clone, Serialize, Deserialize)]
struct Signup {
    email: String,
    name: String,
    property_ids: Vec<String>,
    maintenance_requests: Vec<MaintenanceRequest>,
    tenant_documents: Vec<TenantDocument>,
    inspection_reminders: Vec<InspectionReminder>,
}

fn valid_portfolio(s: &Signup) -> bool {
    !s.email.trim().is_empty() && !s.name.trim().is_empty()
        && !s.property_ids.is_empty()
        && s.property_ids.iter().all(|id| !id.trim().is_empty())
        && s.maintenance_requests.iter().all(|r| s.property_ids.contains(&r.property_id))
        && s.tenant_documents.iter().all(|d| s.property_ids.contains(&d.property_id))
        && s.inspection_reminders.iter().all(|r| s.property_ids.contains(&r.property_id))
}

#[derive(Clone)]
struct Service {
    infrai: InfraiClient,
    public_url: String,
    pending: Arc<Mutex<HashMap<String, Signup>>>,
}

enum ServiceError {
    BadRequest(&'static str),
    NotFound,
    Upstream(InfraiError),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m.to_owned()),
            Self::NotFound => (StatusCode::NOT_FOUND, "Verification link not found".into()),
            Self::Upstream(InfraiError::Rejected { status, message, .. }) if (400..500).contains(&status) =>
                (StatusCode::BAD_REQUEST, message),
            Self::Upstream(error) => {
                eprintln!("upstream request: {error}");
                (StatusCode::BAD_GATEWAY, "Upstream request could not be completed".into())
            }
        };
        (status, Json(json!({"error": message}))).into_response()
    }
}

async fn signup(State(service): State<Service>, Json(input): Json<Signup>)
    -> Result<(StatusCode, Json<Value>), ServiceError> {
    if !valid_portfolio(&input) {
        return Err(ServiceError::BadRequest("Each record must reference a registered property"));
    }
    let token = Uuid::new_v4().to_string();
    let link = format!("{}/verify/{}", service.public_url.trim_end_matches('/'), token);
    let message_id = service.infrai.send_link(&input.email, &link, &token)
        .await.map_err(ServiceError::Upstream)?;
    service.pending.lock().await.insert(token, input);
    Ok((StatusCode::ACCEPTED, Json(json!({"message_id": message_id, "state": "email_sent"}))))
}

async fn verify(State(service): State<Service>, Path(token): Path<String>)
    -> Result<Json<Value>, ServiceError> {
    let input = service.pending.lock().await.remove(&token).ok_or(ServiceError::NotFound)?;
    let outcome = async {
        let user_id = service.infrai.create_user(&input.email, &input.name, &token).await?;
        let session = service.infrai.create_session(&user_id, &token).await?;
        Ok::<_, InfraiError>((user_id, session))
    }.await;
    match outcome {
        Ok((user_id, session)) => Ok(Json(json!({
            "state": "verified", "user_id": user_id, "session": session,
            "property_ids": input.property_ids,
            "maintenance_requests": input.maintenance_requests,
            "tenant_documents": input.tenant_documents,
            "inspection_reminders": input.inspection_reminders,
        }))),
        Err(error) => {
            service.pending.lock().await.insert(token, input);
            Err(ServiceError::Upstream(error))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = Service {
        infrai: InfraiClient::from_env()?,
        public_url: env::var("PUBLIC_URL")?,
        pending: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = Router::new().route("/signup", post(signup))
        .route("/verify/:token", get(verify)).with_state(service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_cannot_target_another_property() {
        let mut signup = Signup {
            email: "manager@example.com".into(), name: "Manager".into(),
            property_ids: vec!["building-7".into()],
            maintenance_requests: vec![MaintenanceRequest {
                property_id: "building-7".into(), summary: "Leak".into(),
            }],
            tenant_documents: vec![],
            inspection_reminders: vec![InspectionReminder {
                property_id: "building-9".into(), due_date: "2027-04-01".into(),
            }],
        };
        assert!(!valid_portfolio(&signup));
        signup.inspection_reminders[0].property_id = "building-7".into();
        assert!(valid_portfolio(&signup));
    }
}
