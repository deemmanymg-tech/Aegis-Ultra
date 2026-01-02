use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use crate::config::AppState;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPayload { pub intent_hash: String, pub policy_hash: String, pub expires_at_unix: i64, pub scope: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalToken { pub payload: ApprovalPayload, pub sig_b64: String }
pub fn verify(_tok: &ApprovalToken, _vk_b64: &str) -> bool { false }
#[derive(Debug, Deserialize)]
pub struct DevSignReq { pub intent_hash: String, pub policy_hash: String, pub scope: String, pub ttl_seconds: i64 }
pub async fn sign_dev_approval(State(_st): State<AppState>, Json(_req): Json<DevSignReq>) -> (StatusCode, Json<serde_json::Value>) {
  (StatusCode::NOT_FOUND, Json(serde_json::json!({"error":"dev signer disabled in v0.1 stub"})))
}