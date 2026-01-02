use crate::{config::AppState, tools::ToolIntent};
pub async fn run(_st: &AppState, _request_id: &str, _intent: &ToolIntent) -> Result<(), String> { Ok(()) }