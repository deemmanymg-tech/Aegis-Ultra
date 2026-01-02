pub mod registry;
pub mod sandbox;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use crate::config::AppState;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk { pub class: String, pub money_usd: i64, pub destructive: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParams { pub tool_id: String, pub args: Vec<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolIntent { pub action: String, pub params: ToolParams, pub risk: Risk, pub constraints: serde_json::Value, pub ticket: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareRecord { pub request_id: String, pub prepare_digest: String, pub intent: ToolIntent }
#[derive(Debug, Deserialize)]
pub struct PrepareReq { pub intent: ToolIntent }
pub async fn prepare(State(st): State<AppState>, Json(req): Json<PrepareReq>) -> (StatusCode, Json<serde_json::Value>) {
  let request_id = uuid::Uuid::new_v4().to_string();
  let allowlisted = st.tool_registry.is_allowlisted(&req.intent.params.tool_id, &req.intent.params.args);
  st.ledger.append("tool.prepare", &request_id, serde_json::json!({"allowlisted":allowlisted}));
  (StatusCode::OK, Json(serde_json::json!({"request_id":request_id,"prepare_digest":"stub"})))
}
#[derive(Debug, Deserialize)]
pub struct CommitReq { pub request_id: String }
pub async fn commit(State(st): State<AppState>, Json(req): Json<CommitReq>) -> (StatusCode, Json<serde_json::Value>) {
  st.ledger.append("tool.commit.denied", &req.request_id, serde_json::json!({"reason":"v0.1 stub"}));
  (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"v0.1 stub"})))
}