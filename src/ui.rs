use axum::response::{Html, IntoResponse};
use axum::Json;
use serde_json::json;

pub async fn index() -> impl IntoResponse {
    Html(r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <title>Aegis Ultra — Dominance v1</title>
  <style>
    body{font-family:system-ui,Segoe UI,Arial;margin:40px;max-width:900px}
    code,pre{background:#f5f5f5;padding:2px 6px;border-radius:6px}
    .card{border:1px solid #ddd;border-radius:12px;padding:16px;margin:12px 0}
    .ok{color:#0a7}
  </style>
</head>
<body>
  <h1>Aegis Ultra — <span class="ok">Dominance v1</span></h1>
  <p>Tool Firewall + Approvals + Bundle Evidence</p>

  <div class="card">
    <h2>Health</h2>
    <p><code>GET /healthz</code></p>
  </div>

  <div class="card">
    <h2>Core API</h2>
    <ul>
      <li><code>POST /v1/tools/prepare</code></li>
      <li><code>POST /v1/tools/commit</code></li>
      <li><code>POST /v1/approvals/sign</code> (dev signer)</li>
      <li><code>GET /v1/aegis/bundle/&lt;request_id&gt;</code></li>
    </ul>
  </div>

  <div class="card">
    <h2>Quick proof</h2>
    <p>Run: <code>powershell -ExecutionPolicy Bypass -File .\scripts\SMOKE_DOMINANCE.ps1</code></p>
    <p>Then open the bundle zip from your temp folder.</p>
  </div>

  <p style="opacity:.7">Dominance v1.0.0 — local demo</p>
</body>
</html>"#)
}

pub async fn version() -> impl IntoResponse {
    Json(json!({
        "name": "aegis-ultra",
        "release": "dominance-v1",
        "status": "ok"
    }))
}
