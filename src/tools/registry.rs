use std::path::PathBuf;
use crate::config::{Policy, ToolSpec};
#[derive(Clone)]
pub struct ToolRegistry { pub tools: Vec<ToolSpec>, pub artifacts_dir: PathBuf }
impl ToolRegistry {
  pub fn from_policy(policy: &Policy, artifacts_dir: &PathBuf) -> Result<Self, String> { std::fs::create_dir_all(artifacts_dir).map_err(|e| e.to_string())?; Ok(Self { tools: policy.tools.clone(), artifacts_dir: artifacts_dir.clone() }) }
  pub fn find(&self, tool_id: &str) -> Option<ToolSpec> { self.tools.iter().find(|t| t.tool_id == tool_id).cloned() }
  pub fn is_allowlisted(&self, tool_id: &str, args: &[String]) -> bool {
    let Some(spec) = self.find(tool_id) else { return false; };
    let is_windows = cfg!(target_os="windows");
    if is_windows && spec.platform.to_ascii_lowercase() != "windows" { return false; }
    if !is_windows && spec.platform.to_ascii_lowercase() != "linux" { return false; }
    for a in args { if !spec.allowed_arg_prefixes.iter().any(|p| a.starts_with(p)) { return false; } }
    true
  }
}