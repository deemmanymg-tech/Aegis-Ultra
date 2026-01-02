#Requires -Version 5.1
$ErrorActionPreference="Stop"

function Write-Utf8NoBom {
  param([Parameter(Mandatory=$true)][string]$Path,[Parameter(Mandatory=$true)][string]$Content)
  $enc = New-Object System.Text.UTF8Encoding($false)
  $dir = Split-Path -Parent $Path
  if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  [System.IO.File]::WriteAllText($Path, $Content, $enc)
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw "cargo not found on PATH" }
if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
  if (Test-Path ".\scripts\IMPORT_VS_ENV.ps1") { . .\scripts\IMPORT_VS_ENV.ps1 }
}

Write-Host "== Patch: hard deny on injection/secrets before upstream =="
$root = Split-Path $PSScriptRoot -Parent
Write-Utf8NoBom (Join-Path $root "src\\dlp.rs") @'
use regex::Regex;
use serde::{Deserialize, Serialize};
use crate::config::Policy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingKind { Secret, Pii, PromptInjection, Domain }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding { pub kind: FindingKind, pub pattern: String, pub snippet: String }

fn rx(p: &str) -> Regex { Regex::new(p).unwrap() }

pub fn scan_text(text: &str, policy: &Policy) -> Vec<Finding> {
  let mut out = vec![];

  // ---- Secrets (expand later) ----
  if policy.block_on_secrets {
    // OpenAI-style
    for (name, re) in [
      ("openai_key", r"(?i)\bsk-[A-Za-z0-9]{20,}\b"),
      ("aws_access_key", r"\bAKIA[0-9A-Z]{16}\b"),
      ("pem_private_key", r"-----BEGIN (?:RSA|EC|OPENSSH|DSA|PRIVATE) KEY-----"),
    ] {
      let re = rx(re);
      if let Some(m) = re.find(text) {
        out.push(Finding{ kind: FindingKind::Secret, pattern:name.to_string(), snippet:text[m.start()..m.end()].to_string() });
      }
    }
  }

  // ---- Prompt injection (aggressive) ----
  if policy.block_on_injection {
    // High-recall patterns
    for (name, re) in [
      ("ignore_instructions", r"(?is)\b(ignore|disregard|bypass|override)\b.{0,200}\b(instruction|system|policy|rules)\b"),
      ("reveal_system", r"(?is)\b(reveal|show|print|leak|display)\b.{0,200}\b(system prompt|system message|developer message|hidden)\b"),
      ("role_hijack", r"(?is)\byou are now\b.{0,200}\b(system|developer)\b"),
      ("do_anything_now", r"(?is)\bDAN\b|\bdo anything now\b"),
    ] {
      let re = rx(re);
      if let Some(m) = re.find(text) {
        out.push(Finding{ kind: FindingKind::PromptInjection, pattern:name.to_string(), snippet:text[m.start()..m.end()].to_string() });
      }
    }
  }

  // PII optional (off by default)
  if policy.block_on_pii {
    let re = rx(r"\b\d{3}-\d{2}-\d{4}\b");
    if let Some(m) = re.find(text) {
      out.push(Finding{ kind: FindingKind::Pii, pattern:"ssn_like".to_string(), snippet:text[m.start()..m.end()].to_string() });
    }
  }

  out
}

pub fn redact_text(text: &str, findings: &[Finding]) -> String {
  let mut out = text.to_string();
  for f in findings {
    match f.kind {
      FindingKind::Secret => out = out.replace(&f.snippet, "[REDACTED_SECRET]"),
      FindingKind::Pii => out = out.replace(&f.snippet, "[REDACTED_PII]"),
      _ => {}
    }
  }
  out
}
'@

Write-Utf8NoBom (Join-Path $root "src\\gateway.rs") @'
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
'@

Write-Host "Rebuild..."
if (Test-Path (Join-Path $root "scripts\\BUILD_ALL.ps1")) { & (Join-Path $root "scripts\\BUILD_ALL.ps1") } else { cargo build --release }

Write-Host "Now restart docker compose with --build:"
Write-Host "  docker compose -f .\docker\docker-compose.yml down"
Write-Host "  docker compose -f .\docker\docker-compose.yml up --build"
Write-Host "✅ Patch applied."
