pub mod registry;
pub mod sandbox;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use time::OffsetDateTime;

use crate::{approvals, config::AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk { pub class: String, pub money_usd: i64, pub destructive: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParams { pub tool_id: String, pub args: Vec<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolIntent {
  pub intent_id: Option<String>,
  pub action: String,
  pub params: ToolParams,
  pub risk: Risk,
  pub constraints: serde_json::Value,
  pub ticket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareRecord {
    pub request_id: String,
    pub prepare_digest: String,
    pub intent_hash: String,
    pub policy_hash: String,
    pub intent: ToolIntent,
    pub created_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct PrepareReq { pub intent: ToolIntent }

#[derive(Debug, Serialize)]
pub struct PrepareResp {
    pub request_id: String,
    pub prepare_digest: String,
    pub intent_hash: String,
    pub policy_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct CommitReq {
    pub request_id: String,
    pub prepare_digest: String,
    pub approval: Option<approvals::ApprovalToken>,
}

#[derive(Debug, Serialize)]
pub struct CommitResp {
    pub ok: bool,
    pub request_id: String,
    pub exit_code: i32,
    pub stdout_path: String,
    pub stderr_path: String,
}

fn hash_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn canon(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys { out.insert(k.clone(), canon(&map[&k])); }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(canon).collect()),
        _ => v.clone(),
    }
}

fn canonical_bytes(v: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&canon(v)).unwrap_or_default()
}

fn compute_policy_hash(st: &AppState) -> String {
    hash_sha256(&st.policy_raw)
}

fn compute_intent_hash(intent: &ToolIntent) -> String {
    let v = serde_json::to_value(intent).unwrap_or(serde_json::json!({}));
    hash_sha256(&canonical_bytes(&v))
}

fn compute_prepare_digest(intent_hash: &str, policy_hash: &str, constraints: &serde_json::Value, created_at: i64) -> String {
    let body = serde_json::json!({
        "intent_hash": intent_hash,
        "policy_hash": policy_hash,
        "constraints": constraints,
        "created_at": created_at
    });
    hash_sha256(&canonical_bytes(&body))
}

fn approval_required(intent: &ToolIntent, policy: &crate::config::Policy) -> bool {
    intent.risk.class == "high" || intent.risk.money_usd >= policy.risk_money_threshold_usd || intent.risk.destructive || policy.risk_high_requires_approval
}

async fn write_decision_file(st: &AppState, request_id: &str, decision: serde_json::Value) {
    let dir = st.tool_registry.artifacts_dir.join(request_id);
    let _ = tokio::fs::create_dir_all(&dir).await;
    let p = dir.join("decision.json");
    let _ = tokio::fs::write(p, serde_json::to_vec_pretty(&decision).unwrap_or_default()).await;
}

pub async fn prepare(State(st): State<AppState>, Json(req): Json<PrepareReq>) -> (StatusCode, Json<serde_json::Value>) {
    if st.policy.tool_prepare_allows_execution {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"policy invalid: prepare cannot execute"})));
    }

    let request_id = Uuid::new_v4().to_string();
    let policy_hash = compute_policy_hash(&st);
    let intent_hash = compute_intent_hash(&req.intent);
    let created_at = OffsetDateTime::now_utc().unix_timestamp();
    let prepare_digest = compute_prepare_digest(&intent_hash, &policy_hash, &req.intent.constraints, created_at);

    let allowlisted = st.tool_registry.is_allowlisted(&req.intent.params.tool_id, &req.intent.params.args);
    if st.policy.fail_closed && !allowlisted {
        st.ledger.append("tool.prepare.denied", &request_id, serde_json::json!({"reason":"not_allowlisted"}));
        write_decision_file(&st, &request_id, serde_json::json!({"allowed":false,"phase":"prepare","reason":"not_allowlisted"})).await;
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"tool not allowlisted","request_id":request_id})));
    }

    if let Some(opa) = &st.opa {
        let input = serde_json::json!({
            "kind":"tool_prepare",
            "request_id": request_id,
            "tool": { "allowlisted": allowlisted },
            "intent": req.intent,
            "approval": { "valid": false }
        });
        if let Err(e) = opa.require_allow(&st.opa_path, input).await {
            st.ledger.append("tool.prepare.denied", &request_id, serde_json::json!({"reason": e.to_string()}));
            write_decision_file(&st, &request_id, serde_json::json!({"allowed":false,"phase":"prepare","reason":e.to_string()})).await;
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"tool prepare denied","request_id":request_id})));
        }
    }

    {
        let mut map = st.prepares.write().await;
        map.insert(request_id.clone(), PrepareRecord {
            request_id: request_id.clone(),
            prepare_digest: prepare_digest.clone(),
            intent_hash: intent_hash.clone(),
            policy_hash: policy_hash.clone(),
            intent: req.intent.clone(),
            created_at,
        });
    }

    st.ledger.append("tool.prepare", &request_id, serde_json::json!({
        "prepare_digest": prepare_digest,
        "intent_hash": intent_hash,
        "policy_hash": policy_hash,
        "allowlisted": allowlisted
    }));

    write_decision_file(&st, &request_id, serde_json::json!({
        "allowed": true,
        "phase": "prepare",
        "intent_hash": intent_hash,
        "policy_hash": policy_hash,
        "prepare_digest": prepare_digest
    })).await;

    (StatusCode::OK, Json(serde_json::json!(PrepareResp { request_id, prepare_digest, intent_hash, policy_hash })))
}

pub async fn commit(State(st): State<AppState>, Json(req): Json<CommitReq>) -> (StatusCode, Json<serde_json::Value>) {
    let rec = {
        let map = st.prepares.read().await;
        map.get(&req.request_id).cloned()
    };
    let Some(rec) = rec else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error":"unknown request_id"})));
    };

    if rec.prepare_digest != req.prepare_digest {
        st.ledger.append("tool.commit.denied", &req.request_id, serde_json::json!({"reason":"prepare_digest_mismatch"}));
        write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":false,"phase":"commit","reason":"prepare_digest_mismatch"})).await;
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"prepare digest mismatch"})));
    }

    let policy_hash = compute_policy_hash(&st);
    let intent_hash_val = rec.intent_hash.clone();
    let recomputed = compute_prepare_digest(&intent_hash_val, &policy_hash, &rec.intent.constraints, rec.created_at);
    if recomputed != req.prepare_digest {
        st.ledger.append("tool.commit.denied", &req.request_id, serde_json::json!({"reason":"intent_or_policy_changed"}));
        write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":false,"phase":"commit","reason":"intent_or_policy_changed"})).await;
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"intent/policy changed"})));
    }

    let allowlisted = st.tool_registry.is_allowlisted(&rec.intent.params.tool_id, &rec.intent.params.args);
    if st.policy.fail_closed && !allowlisted {
        st.ledger.append("tool.commit.denied", &req.request_id, serde_json::json!({"reason":"not_allowlisted"}));
        write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":false,"phase":"commit","reason":"not_allowlisted"})).await;
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"tool not allowlisted"})));
    }

    let needs_approval = approval_required(&rec.intent, &st.policy);
    let mut approval_valid = false;
    if needs_approval {
        if let Some(tok) = &req.approval {
            approval_valid = tok.payload.intent_hash == rec.intent_hash
                && tok.payload.policy_hash == rec.policy_hash
                && approvals::verify(tok, &st.policy.approval.verifying_key_b64)
                && tok.payload.scope == rec.intent.params.tool_id;
        }
        if !approval_valid {
            st.ledger.append("tool.commit.denied", &req.request_id, serde_json::json!({"reason":"approval_required"}));
            write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":false,"phase":"commit","reason":"approval_required"})).await;
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"approval required"})));
        }
    }

    if let Some(opa) = &st.opa {
        let input = serde_json::json!({
            "kind":"tool_commit",
            "request_id": req.request_id,
            "tool": { "allowlisted": allowlisted },
            "intent": rec.intent,
            "approval": { "valid": approval_valid }
        });
        if let Err(e) = opa.require_allow(&st.opa_path, input).await {
            st.ledger.append("tool.commit.denied", &req.request_id, serde_json::json!({"reason": e.to_string()}));
            write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":false,"phase":"commit","reason":e.to_string()})).await;
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"tool commit denied"})));
        }
    }

    let out = crate::tools::sandbox::native::run(&st, &req.request_id, &rec.intent).await;
    match out {
        Ok(r) => {
            st.ledger.append("tool.commit", &req.request_id, serde_json::json!({
                "exit_code": r.exit_code,
                "stdout_path": r.stdout_path,
                "stderr_path": r.stderr_path
            }));
            write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":true,"phase":"commit","exit_code":r.exit_code})).await;
            (StatusCode::OK, Json(serde_json::json!(r)))
        }
        Err(e) => {
            st.ledger.append("tool.commit.error", &req.request_id, serde_json::json!({"error": e}));
            write_decision_file(&st, &req.request_id, serde_json::json!({"allowed":false,"phase":"commit","reason":"exec_failed","detail":e})).await;
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error":"execution failed"})))
        }
    }
}
