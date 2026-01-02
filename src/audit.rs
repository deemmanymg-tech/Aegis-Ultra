use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::OpenOptions, io::Write, path::{Path, PathBuf}, sync::Mutex};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent { pub event_type: String, pub request_id: String, pub payload: serde_json::Value, pub prev_hash: String, pub hash: String }
pub struct AuditLedger { path: PathBuf, state: Mutex<String> }
impl AuditLedger {
  pub fn new(path: &Path) -> Self { Self { path: path.to_path_buf(), state: Mutex::new("GENESIS".to_string()) } }
  pub fn append(&self, event_type: &str, request_id: &str, payload: serde_json::Value) {
    let mut prev = self.state.lock().unwrap();
    let body = serde_json::json!({"event_type":event_type,"request_id":request_id,"payload":payload,"prev_hash":*prev});
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let mut h = Sha256::new(); h.update(&bytes);
    let hash = hex::encode(h.finalize());
    let ev = AuditEvent{ event_type:event_type.to_string(), request_id:request_id.to_string(), payload, prev_hash:prev.clone(), hash:hash.clone() };
    *prev = hash;
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.path) { let _ = writeln!(f, "{}", serde_json::to_string(&ev).unwrap_or_default()); }
  }
  pub fn export_all(&self) -> String { std::fs::read_to_string(&self.path).unwrap_or_default() }
}
