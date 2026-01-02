use axum::{
  extract::State,
  http::{HeaderMap, StatusCode},
  response::IntoResponse,
  Json,
};
use uuid::Uuid;

use crate::{config::AppState, dlp};

pub async fn healthz() -> impl IntoResponse { (StatusCode::OK, "ok") }

pub async fn export_audit(State(st): State<AppState>) -> impl IntoResponse {
  (StatusCode::OK, st.ledger.export_all())
}

pub async fn chat_completions(
  State(st): State<AppState>,
  headers: HeaderMap,
  Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
  let request_id = Uuid::new_v4().to_string();

  // scan request
  let raw = serde_json::to_string(&req).unwrap_or_default();
  let findings = dlp::scan_text(&raw, &st.policy);
  st.ledger.append("prompt.scan", &request_id, serde_json::json!({"findings": findings}));

  // HARD LOCAL DENY (always before upstream)
  for f in &findings {
    match f.kind {
      dlp::FindingKind::Secret if st.policy.block_on_secrets => {
        st.ledger.append("prompt.deny", &request_id, serde_json::json!({"reason":"secrets_detected"}));
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"denied","reason":"secrets_detected","request_id":request_id}))).into_response();
      }
      dlp::FindingKind::PromptInjection if st.policy.block_on_injection => {
        st.ledger.append("prompt.deny", &request_id, serde_json::json!({"reason":"prompt_injection"}));
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"denied","reason":"prompt_injection","request_id":request_id}))).into_response();
      }
      dlp::FindingKind::Pii if st.policy.block_on_pii => {
        st.ledger.append("prompt.deny", &request_id, serde_json::json!({"reason":"pii_detected"}));
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"denied","reason":"pii_detected","request_id":request_id}))).into_response();
      }
      _ => {}
    }
  }

  // If not denied locally, optionally enforce OPA (future hardening).
  // Forward upstream (may 502 if upstream absent; that's fine for non-deny traffic)
  let auth = headers.get("authorization").and_then(|v| v.to_str().ok());

  // upstream call
  let url = format!("{}/v1/chat/completions", st.policy.upstream_base_url.trim_end_matches('/'));
  let http = reqwest::Client::new();
  let mut r = http.post(url).json(&req);
  if let Some(a) = auth { r = r.header("Authorization", a); }

  match r.send().await {
    Ok(res) => {
      if (!res.status().is_success()) {
        st.ledger.append("upstream.error", &request_id, serde_json::json!({"status": res.status().as_u16()}));
        return (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":"upstream error","request_id":request_id}))).into_response();
      }
      match res.json::<serde_json::Value>().await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(_) => (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":"upstream decode error","request_id":request_id}))).into_response(),
      }
    }
    Err(e) => {
      st.ledger.append("upstream.error", &request_id, serde_json::json!({"error": e.to_string()}));
      (StatusCode::BAD_GATEWAY, Json(serde_json::json!({"error":"upstream error","request_id":request_id}))).into_response()
    }
  }
}