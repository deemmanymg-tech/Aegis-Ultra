use std::process::Stdio;
use tokio::process::Command;

use crate::{config::AppState, tools::{ToolIntent, CommitResp}};

fn is_absolute(p: &str) -> bool {
    if cfg!(target_os="windows") {
        (p.len() > 2 && p.as_bytes()[1] == b':' && (p.as_bytes()[2] == b'\\' || p.as_bytes()[2] == b'/'))
            || p.starts_with("\\\\")
    } else {
        p.starts_with("/")
    }
}

pub async fn run(st: &AppState, request_id: &str, intent: &ToolIntent) -> Result<CommitResp, String> {
    let spec = st.tool_registry.find(&intent.params.tool_id).ok_or("unknown tool_id")?;
    if !is_absolute(&spec.executable) {
        return Err("tool executable must be absolute path".to_string());
    }

    let dir = st.tool_registry.artifacts_dir.join(request_id);
    let workdir = dir.join("work");
    tokio::fs::create_dir_all(&workdir).await.map_err(|e| e.to_string())?;

    let stdout_path = dir.join("stdout.txt");
    let stderr_path = dir.join("stderr.txt");
    let decision_path = dir.join("decision.json");

    let mut cmd = Command::new(&spec.executable);
    cmd.args(&intent.params.args);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.current_dir(&workdir);
    cmd.env_clear();

    if let Ok(path) = std::env::var("AEGIS_SANDBOX_PATH") {
        cmd.env("PATH", path);
    }

    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    let exit_code = out.status.code().unwrap_or(-1);
    let stdout_bytes = out.stdout;
    let stderr_bytes = out.stderr;
    let timed_out = false;

    tokio::fs::write(&stdout_path, &stdout_bytes).await.map_err(|e| e.to_string())?;
    tokio::fs::write(&stderr_path, &stderr_bytes).await.map_err(|e| e.to_string())?;

    let decision = serde_json::json!({
        "allowed": true,
        "timed_out": timed_out,
        "exit_code": exit_code,
        "tool_id": intent.params.tool_id,
        "args": intent.params.args,
    });
    tokio::fs::write(&decision_path, serde_json::to_vec_pretty(&decision).unwrap_or_default()).await.map_err(|e| e.to_string())?;

    Ok(CommitResp{
        ok: !timed_out && exit_code == 0,
        request_id: request_id.to_string(),
        exit_code,
        stdout_path: stdout_path.to_string_lossy().to_string(),
        stderr_path: stderr_path.to_string_lossy().to_string(),
    })
}
