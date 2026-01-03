use serde_json::Value;
use thiserror::Error;
#[derive(Debug, Error)]
pub enum OpaError {
    #[error("OPA http error: {0}")]
    Http(String),
    #[error("OPA denied: {0}")]
    Denied(String),
}
#[derive(Clone)]
pub struct OpaClient {
    base: String,
    http: reqwest::Client,
}
impl OpaClient {
    pub fn new(base: String) -> Self {
        Self {
            base,
            http: reqwest::Client::new(),
        }
    }
    pub async fn require_allow(&self, data_path: &str, input: Value) -> Result<Value, OpaError> {
        let url = format!(
            "{}/v1/data/{}",
            self.base.trim_end_matches('/'),
            data_path.trim_start_matches('/')
        );
        let res = self
            .http
            .post(url)
            .json(&serde_json::json!({"input":input}))
            .send()
            .await
            .map_err(|e| OpaError::Http(e.to_string()))?;
        let status = res.status();
        let v: Value = res
            .json()
            .await
            .map_err(|e| OpaError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(OpaError::Http(format!("status={} body={}", status, v)));
        }
        let allow = v
            .pointer("/result/allow")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        if !allow {
            let reason = v
                .pointer("/result/reason")
                .and_then(|x| x.as_str())
                .unwrap_or("policy_denied");
            return Err(OpaError::Denied(reason.to_string()));
        }
        Ok(v)
    }
}
