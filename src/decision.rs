use serde::Serialize;
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[allow(dead_code)]
#[derive(Serialize)]
pub struct DecisionRecord<'a> {
    pub ts_unix_ms: u128,
    pub request_id: &'a str,
    pub tool: &'a str,
    pub exec: &'a str,
    pub argv: &'a [String],
    pub allow: bool,
    pub reason: &'a str,
    pub policy: &'a str,
    pub digest: Option<&'a str>,
}

#[allow(dead_code)]
pub fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[allow(dead_code)]
pub fn write_decision_json(out_dir: &Path, record: &DecisionRecord<'_>) -> std::io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let p = out_dir.join("decision.json");
    let json = serde_json::to_string_pretty(record).unwrap_or_else(|_| "{}".to_string());
    fs::write(p, json.as_bytes())
}
