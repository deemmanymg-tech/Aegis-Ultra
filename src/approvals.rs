use axum::{extract::State, http::StatusCode, Json};
use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::config::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPayload {
    pub intent_hash: String,
    pub policy_hash: String,
    pub expires_at_unix: i64,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalToken {
    pub payload: ApprovalPayload,
    pub sig_b64: String,
}

fn canonical_payload_bytes(p: &ApprovalPayload) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
      "intent_hash": p.intent_hash,
      "policy_hash": p.policy_hash,
      "expires_at_unix": p.expires_at_unix,
      "scope": p.scope
    }))
    .unwrap_or_default()
}

fn derive_verifying_from_env() -> Option<VerifyingKey> {
    if let Ok(sk_b64) = std::env::var("AEGIS_OPERATOR_SK_B64") {
        if let Ok(bytes) = general_purpose::STANDARD.decode(sk_b64) {
            if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
                let sk = SigningKey::from_bytes(&arr);
                return Some(sk.verifying_key());
            }
        }
    }
    None
}

pub fn verify(token: &ApprovalToken, verifying_key_b64: &str) -> bool {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if token.payload.expires_at_unix < now {
        return false;
    }

    let maybe_vk = if verifying_key_b64.trim().is_empty() {
        derive_verifying_from_env()
    } else {
        general_purpose::STANDARD
            .decode(verifying_key_b64)
            .ok()
            .and_then(|b| VerifyingKey::from_bytes(&b.try_into().ok()?).ok())
    };

    if let Some(vk) = maybe_vk {
        let sig_bytes = match general_purpose::STANDARD.decode(&token.sig_b64) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let sig = match Signature::from_slice(&sig_bytes) {
            Ok(s) => s,
            Err(_) => return false,
        };
        return vk
            .verify_strict(&canonical_payload_bytes(&token.payload), &sig)
            .is_ok();
    }

    std::env::var("AEGIS_DEV_SIGNER").unwrap_or_else(|_| "0".to_string()) == "1"
}

#[derive(Debug, Deserialize)]
pub struct DevSignReq {
    pub intent_hash: String,
    pub policy_hash: String,
    pub scope: String,
    pub ttl_seconds: i64,
}

pub async fn sign_dev_approval(
    State(st): State<AppState>,
    Json(req): Json<DevSignReq>,
) -> (StatusCode, Json<serde_json::Value>) {
    let enabled = std::env::var("AEGIS_DEV_SIGNER").unwrap_or_else(|_| "0".to_string()) == "1";
    if !enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error":"dev signer disabled"})),
        );
    }

    let sk_b64 = match std::env::var("AEGIS_OPERATOR_SK_B64") {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"AEGIS_OPERATOR_SK_B64 missing"})),
            )
        }
    };

    let sk_bytes = match general_purpose::STANDARD.decode(sk_b64) {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error":"bad operator sk b64"})),
            )
        }
    };
    if sk_bytes.len() != 32 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"operator sk must be 32 bytes"})),
        );
    }
    let sk = SigningKey::from_bytes(&sk_bytes.try_into().unwrap());

    let expires = OffsetDateTime::now_utc().unix_timestamp() + req.ttl_seconds.clamp(30, 86400);
    let payload = ApprovalPayload {
        intent_hash: req.intent_hash,
        policy_hash: req.policy_hash,
        expires_at_unix: expires,
        scope: req.scope,
    };
    let sig: Signature = sk.sign(&canonical_payload_bytes(&payload));
    let tok = ApprovalToken {
        payload,
        sig_b64: general_purpose::STANDARD.encode(sig.to_bytes()),
    };

    st.ledger.append(
        "approval.sign",
        "DEV",
        serde_json::json!({"scope": tok.payload.scope, "expires": tok.payload.expires_at_unix}),
    );
    (StatusCode::OK, Json(serde_json::json!({"token": tok})))
}
