use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::io::Write;
use time::OffsetDateTime;
use uuid::Uuid;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{config::AppState, dlp, opa::OpaError};

#[derive(Clone)]
pub struct UpstreamClient {
    base: String,
    http: reqwest::Client,
}
impl UpstreamClient {
    pub fn new(base: String) -> Self {
        Self {
            base,
            http: reqwest::Client::new(),
        }
    }
    pub async fn forward_chat(
        &self,
        body: serde_json::Value,
        auth: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let url = format!("{}/v1/chat/completions", self.base.trim_end_matches('/'));
        let mut r = self.http.post(url).json(&body);
        if let Some(a) = auth {
            r = r.header("Authorization", a);
        }
        let res = r.send().await.map_err(|e| e.to_string())?;
        if !res.status().is_success() {
            return Err(format!("upstream status {}", res.status()));
        }
        res.json::<serde_json::Value>()
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Threat {
    pub id: String,
    pub ts: String,
    pub severity: String,
    pub rule: String,
    pub src_ip: String,
    pub dst_ip: String,
    pub action: String,
    pub reason: String,
}

fn demo_threats() -> Vec<Threat> {
    vec![
        Threat {
            id: "t-1001".into(),
            ts: "2026-01-02T18:00:00Z".into(),
            severity: "critical".into(),
            rule: "Deny: Prompt Injection".into(),
            src_ip: "10.0.0.10".into(),
            dst_ip: "127.0.0.1".into(),
            action: "blocked".into(),
            reason: "policy_denied".into(),
        },
        Threat {
            id: "t-1002".into(),
            ts: "2026-01-02T17:45:00Z".into(),
            severity: "high".into(),
            rule: "Deny: Secrets".into(),
            src_ip: "10.0.0.5".into(),
            dst_ip: "127.0.0.1".into(),
            action: "blocked".into(),
            reason: "secret_detected".into(),
        },
        Threat {
            id: "t-1003".into(),
            ts: "2026-01-02T17:30:00Z".into(),
            severity: "medium".into(),
            rule: "Deny: Domain".into(),
            src_ip: "192.168.1.4".into(),
            dst_ip: "127.0.0.1".into(),
            action: "blocked".into(),
            reason: "domain_not_allowlisted".into(),
        },
    ]
}

#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WindowQuery {
    pub window: Option<String>,
}

pub async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

pub async fn readyz(State(st): State<AppState>) -> impl IntoResponse {
    if let Some(opa) = &st.opa {
        let input = serde_json::json!({"kind":"ready"});
        if let Err(e) = opa.require_allow(&st.opa_path, input).await {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"ok":false,"reason":e.to_string()})),
            );
        }
    }
    (StatusCode::OK, Json(serde_json::json!({"ok":true})))
}

pub async fn api_health(State(st): State<AppState>) -> impl IntoResponse {
    let uptime_ms = (OffsetDateTime::now_utc() - st.started_at).whole_milliseconds();
    (
        StatusCode::OK,
        Json(serde_json::json!({
          "ok": true,
          "version": "1.0.0",
          "uptimeMs": uptime_ms,
        })),
    )
}

pub async fn api_status(State(st): State<AppState>) -> impl IntoResponse {
    let live = st.threats.read().await.clone();
    let threats: Vec<Threat> = if live.is_empty() {
        demo_threats()
    } else {
        live.into_iter().collect()
    };
    let blocked = threats.len() as u64;
    let alerts = threats
        .iter()
        .filter(|t| t.severity == "critical" || t.severity == "high")
        .count() as u64;
    let uptime_ms = (OffsetDateTime::now_utc() - st.started_at).whole_milliseconds();
    (
        StatusCode::OK,
        Json(serde_json::json!({
          "blockedThreats": blocked,
          "alerts": alerts,
          "uptimeMs": uptime_ms,
          "policy": { "failClosed": st.policy.fail_closed },
          "auth": if st.auth_token.is_some() { "token" } else { "open" }
        })),
    )
}

pub async fn api_threats(
    State(st): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> impl IntoResponse {
    let live: Vec<Threat> = st.threats.read().await.clone().into_iter().collect();
    let mut items = if live.is_empty() {
        demo_threats()
    } else {
        live
    };
    if let Some(lim) = q.limit {
        if lim < items.len() {
            items.truncate(lim);
        }
    }
    (StatusCode::OK, Json(items))
}

pub async fn api_threats_summary(Query(q): Query<WindowQuery>) -> impl IntoResponse {
    let _window = q.window.unwrap_or_else(|| "24h".into());
    let threats = demo_threats();
    let mut sev = std::collections::HashMap::new();
    for t in threats {
        *sev.entry(t.severity).or_insert(0u64) += 1;
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({
          "bySeverity": sev,
          "window": "24h"
        })),
    )
}

pub async fn api_audit(
    State(st): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(50);
    let content = st.ledger.export_all();
    let mut lines: Vec<_> = content.lines().rev().take(limit).collect();
    lines.reverse();
    (StatusCode::OK, Json(serde_json::json!({ "lines": lines })))
}

pub async fn export_audit(State(st): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, st.ledger.export_all())
}

fn opa_fail_closed(st: &AppState, e: &OpaError) -> bool {
    match e {
        OpaError::Denied(_) => true,
        OpaError::Http(_) => st.policy.fail_closed,
    }
}

fn record_threat(st: &AppState, sev: &str, rule: &str, reason: &str) {
    let mut buf = st.threats.blocking_write();
    if buf.len() > 999 {
        buf.pop_front();
    }
    buf.push_back(Threat {
        id: format!("t-{}", Uuid::new_v4()),
        ts: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "now".into()),
        severity: sev.to_string(),
        rule: rule.to_string(),
        src_ip: "127.0.0.1".to_string(),
        dst_ip: "127.0.0.1".to_string(),
        action: "blocked".to_string(),
        reason: reason.to_string(),
    });
}

pub async fn chat_completions(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<serde_json::Value>,
) -> impl IntoResponse {
    let request_id = Uuid::new_v4().to_string();

    let raw = serde_json::to_string(&req).unwrap_or_default();
    let findings = dlp::scan_text(&raw, &st.policy);
    st.ledger.append(
        "prompt.scan",
        &request_id,
        serde_json::json!({"findings": findings}),
    );

    for f in &findings {
        match f.kind {
            dlp::FindingKind::Secret if st.policy.block_on_secrets => {
                st.ledger.append(
                    "prompt.deny",
                    &request_id,
                    serde_json::json!({"reason":"secrets_detected"}),
                );
                record_threat(&st, "high", "Deny: Secrets", "secret_detected");
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"denied","reason":"secrets_detected","request_id":request_id}))).into_response();
            }
            dlp::FindingKind::PromptInjection if st.policy.block_on_injection => {
                st.ledger.append(
                    "prompt.deny",
                    &request_id,
                    serde_json::json!({"reason":"prompt_injection"}),
                );
                record_threat(
                    &st,
                    "critical",
                    "Deny: Prompt Injection",
                    "prompt_injection",
                );
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"denied","reason":"prompt_injection","request_id":request_id}))).into_response();
            }
            dlp::FindingKind::Pii if st.policy.block_on_pii => {
                st.ledger.append(
                    "prompt.deny",
                    &request_id,
                    serde_json::json!({"reason":"pii_detected"}),
                );
                record_threat(&st, "medium", "Deny: PII", "pii_detected");
                return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error":"denied","reason":"pii_detected","request_id":request_id}))).into_response();
            }
            _ => {}
        }
    }

    if let Some(opa) = &st.opa {
        let input =
            serde_json::json!({"kind":"prompt","request_id":request_id,"findings":findings});
        if let Err(e) = opa.require_allow(&st.opa_path, input).await {
            st.ledger.append(
                "prompt.denied",
                &request_id,
                serde_json::json!({"reason": e.to_string()}),
            );
            if opa_fail_closed(&st, &e) {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({"error":"Blocked by policy","request_id":request_id})),
                )
                    .into_response();
            }
        }
    }

    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());

    match st.upstream.forward_chat(req, auth).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => {
            st.ledger.append(
                "upstream.error",
                &request_id,
                serde_json::json!({"error": e}),
            );
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error":"upstream error","request_id":request_id})),
            )
                .into_response()
        }
    }
}

pub async fn support_bundle(
    axum::extract::State(st): axum::extract::State<AppState>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    // pull request_id from extensions if present
    let req_id = req
        .extensions()
        .get::<crate::RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_default();
    let mut buf: Vec<u8> = vec![];
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        // redact policy fields that look like secrets
        fn redact(val: &mut serde_json::Value) {
            match val {
                serde_json::Value::Object(map) => {
                    for (k, v) in map.iter_mut() {
                        let kl = k.to_ascii_lowercase();
                        if kl.contains("token")
                            || kl.contains("key")
                            || kl.contains("secret")
                            || kl.contains("authorization")
                        {
                            *v = serde_json::Value::String("[REDACTED]".into());
                        } else {
                            redact(v);
                        }
                    }
                }
                serde_json::Value::Array(arr) => {
                    for v in arr.iter_mut() {
                        redact(v);
                    }
                }
                _ => {}
            }
        }
        let mut policy_val = serde_json::to_value(&*st.policy).unwrap_or(serde_json::json!({}));
        redact(&mut policy_val);
        let policy_json = serde_json::to_string_pretty(&policy_val).unwrap_or_default();
        let _ = zip.start_file("policy_snapshot.json", opts);
        let _ = zip.write(policy_json.as_bytes());

        let audit_raw = st.ledger.export_all();
        let audit = audit_raw
            .lines()
            .map(|ln| ln.replace("Authorization", "Authorization: [REDACTED]"))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = zip.start_file("audit.jsonl", opts);
        let _ = zip.write(audit.as_bytes());

        let threats: Vec<Threat> = st.threats.read().await.clone().into_iter().collect();
        let threats_json = serde_json::to_string_pretty(&threats).unwrap_or_default();
        let _ = zip.start_file("threats.json", opts);
        let _ = zip.write(threats_json.as_bytes());

        let meta = serde_json::json!({
          "request_id": req_id,
          "generated_at": OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| "now".into()),
          "uptime_ms": (OffsetDateTime::now_utc() - st.started_at).whole_milliseconds()
        });
        let _ = zip.start_file("meta.json", opts);
        let _ = zip.write(
            serde_json::to_vec_pretty(&meta)
                .unwrap_or_default()
                .as_slice(),
        );

        let _ = zip.finish();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/zip")
        .header(
            "content-disposition",
            "attachment; filename=\"support_bundle.zip\"",
        )
        .body(axum::body::Body::from(buf))
        .unwrap()
}
