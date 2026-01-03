use crate::{
    audit::AuditLedger, gateway::UpstreamClient, opa::OpaClient, tools::registry::ToolRegistry,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use time::OffsetDateTime;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub tool_id: String,
    pub platform: String,
    pub executable: String,
    pub allowed_arg_prefixes: Vec<String>,
    pub sha256_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalCfg {
    pub verifying_key_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub upstream_base_url: String,
    pub fail_closed: bool,
    pub redact_before_upstream: bool,
    pub redact_response_to_client: bool,
    pub allowed_domains: HashSet<String>,
    pub block_unknown_domains: bool,
    pub block_on_secrets: bool,
    pub block_on_injection: bool,
    pub block_on_pii: bool,
    pub risk_high_requires_approval: bool,
    pub risk_money_threshold_usd: i64,
    pub tool_prepare_allows_execution: bool,
    pub approval: ApprovalCfg,
    pub tools: Vec<ToolSpec>,
}

#[derive(Clone)]
pub struct AppState {
    pub policy: Arc<Policy>,
    pub policy_raw: Arc<Vec<u8>>,
    pub ledger: Arc<AuditLedger>,
    pub opa: Option<Arc<OpaClient>>,
    pub opa_path: String,
    pub upstream: Arc<UpstreamClient>,
    pub tool_registry: Arc<ToolRegistry>,
    pub prepares: Arc<RwLock<std::collections::HashMap<String, crate::tools::PrepareRecord>>>,
    #[allow(dead_code)]
    pub sandbox_timeout_ms: u64,
    pub started_at: OffsetDateTime,
    pub threats: Arc<RwLock<VecDeque<crate::gateway::Threat>>>,
    pub auth_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    policy_path: PathBuf,
    bind: SocketAddr,
    audit_path: PathBuf,
    artifacts_dir: PathBuf,
    upstream_override: Option<String>,
    opa_url: Option<String>,
    opa_path: String,
    sandbox_timeout_ms: u64,
    auth_token: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let policy_path = std::env::var("AEGIS_POLICY_PATH")
            .unwrap_or_else(|_| "policy/packs/policy.json".to_string());
        let bind_s = std::env::var("AEGIS_BIND").unwrap_or_else(|_| "127.0.0.1:8088".to_string());
        let bind: SocketAddr = bind_s
            .parse()
            .map_err(|e: std::net::AddrParseError| e.to_string())?;
        let audit_path =
            std::env::var("AEGIS_AUDIT_PATH").unwrap_or_else(|_| "aegis_audit.jsonl".to_string());
        let artifacts_dir =
            std::env::var("AEGIS_ARTIFACTS_DIR").unwrap_or_else(|_| "artifacts".to_string());
        let upstream_override = std::env::var("AEGIS_UPSTREAM").ok();
        let opa_url = std::env::var("AEGIS_OPA_URL").ok();
        let opa_path =
            std::env::var("AEGIS_OPA_PATH").unwrap_or_else(|_| "aegis/decision/result".to_string());
        let sandbox_timeout_ms = std::env::var("AEGIS_SANDBOX_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30_000);
        let auth_token = std::env::var("AEGIS_UI_TOKEN")
            .ok()
            .filter(|s| !s.trim().is_empty());
        Ok(Self {
            policy_path: PathBuf::from(policy_path),
            bind,
            audit_path: PathBuf::from(audit_path),
            artifacts_dir: PathBuf::from(artifacts_dir),
            upstream_override,
            opa_url,
            opa_path,
            sandbox_timeout_ms,
            auth_token,
        })
    }
    pub fn bind_addr(&self) -> SocketAddr {
        self.bind
    }
    pub async fn build_state(&self) -> Result<AppState, String> {
        let bytes = fs::read(&self.policy_path).map_err(|e| format!("read policy: {}", e))?;
        let mut policy: Policy =
            serde_json::from_slice(&bytes).map_err(|e| format!("parse policy: {}", e))?;
        if let Some(u) = &self.upstream_override {
            policy.upstream_base_url = u.clone();
        }
        let ledger = AuditLedger::new(&self.audit_path);
        let tool_registry = ToolRegistry::from_policy(&policy, &self.artifacts_dir)?;
        let opa = self
            .opa_url
            .as_ref()
            .map(|url| Arc::new(OpaClient::new(url.clone())));
        let upstream = UpstreamClient::new(policy.upstream_base_url.clone());
        let threats = Arc::new(RwLock::new(VecDeque::new()));
        Ok(AppState {
            policy: Arc::new(policy),
            policy_raw: Arc::new(bytes),
            ledger: Arc::new(ledger),
            opa,
            opa_path: self.opa_path.clone(),
            upstream: Arc::new(upstream),
            tool_registry: Arc::new(tool_registry),
            prepares: Arc::new(RwLock::new(std::collections::HashMap::new())),
            sandbox_timeout_ms: self.sandbox_timeout_ms,
            started_at: OffsetDateTime::now_utc(),
            threats,
            auth_token: self.auth_token.clone(),
        })
    }
}
