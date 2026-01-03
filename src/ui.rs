use axum::response::{Html, IntoResponse};
use axum::Json;
use serde_json::json;

pub async fn index() -> impl IntoResponse {
    Html(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <title>Aegis Ultra — Dominance v1</title>
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <style>
    :root {
      --bg0: #0b0d10;
      --bg1: #11141a;
      --bg2: #171b22;
      --card: rgba(23,27,34,.92);
      --border: rgba(255,255,255,.06);
      --text0: #e7eaf0;
      --text1: #aab2c2;
      --green: #42d392;
      --blue: #4aa3ff;
      --amber: #ffb454;
      --red: #ff5c7c;
      --shadow: 0 12px 40px rgba(0,0,0,.55);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Segoe UI", "Inter", system-ui, -apple-system, sans-serif;
      background: radial-gradient(circle at 20% 20%, rgba(66,211,146,.07), transparent 25%), radial-gradient(circle at 80% 0%, rgba(74,163,255,.08), transparent 30%), var(--bg0);
      color: var(--text0);
      display: flex;
      min-height: 100vh;
    }
    .sidebar {
      width: 240px;
      background: var(--bg1);
      border-right: 1px solid var(--border);
      padding: 20px;
      position: sticky;
      top: 0;
      height: 100vh;
    }
    .brand { font-weight: 700; letter-spacing: 0.5px; margin-bottom: 20px; }
    .nav a {
      display: block;
     	color: var(--text1);
      text-decoration: none;
      padding: 8px 10px;
      border-radius: 10px;
      margin: 4px 0;
    }
    .nav a:hover, .nav a.active { background: var(--bg2); color: var(--text0); }
    .main {
      flex: 1;
      padding: 28px;
    }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 14px;
      margin-bottom: 16px;
    }
    .card {
      background: var(--card);
      border: 1px solid var(--border);
      border-radius: 14px;
      padding: 14px;
      box-shadow: var(--shadow);
    }
    .title-row {
      display: flex;
      align-items: center;
      justify-content: space-between;
      margin-bottom: 6px;
    }
    .chip { padding: 2px 10px; border-radius: 999px; font-size: 12px; background: rgba(66,211,146,.12); color: var(--green); }
    table { width: 100%; border-collapse: collapse; color: var(--text0); }
    th, td { padding: 10px 6px; border-bottom: 1px solid var(--border); text-align: left; font-size: 13px; }
    th { color: var(--text1); font-weight: 600; }
    .sev { padding: 2px 8px; border-radius: 999px; font-size: 12px; }
    .sev.critical { background: rgba(255,92,124,.12); color: var(--red); }
    .sev.high { background: rgba(255,180,84,.15); color: var(--amber); }
    .sev.medium { background: rgba(74,163,255,.15); color: var(--blue); }
    .muted { color: var(--text1); font-size: 13px; }
    .link { color: var(--blue); text-decoration: none; font-weight: 600; }
    .link:hover { text-decoration: underline; }
    .kpi { font-size: 26px; font-weight: 700; margin: 4px 0; }
    .row { display: flex; gap: 14px; flex-wrap: wrap; }
    .half { flex: 1 1 360px; }
    .banner { background: linear-gradient(90deg, rgba(255,180,84,.16), rgba(255,92,124,.12)); color: var(--amber); border: 1px solid rgba(255,180,84,.3); padding: 10px 12px; border-radius: 10px; margin: 6px 0 14px 0; font-size: 13px; display:flex; align-items:center; gap:8px; }
    .banner .dot { width:8px; height:8px; border-radius:999px; background: var(--amber); display:inline-block; }
  </style>
</head>
<body>
  <div class="sidebar">
    <div class="brand">Aegis Ultra</div>
    <div class="nav">
      <a class="active" href="#">Dashboard</a>
      <a href="#">Network Protection</a>
      <a href="#">Firewall Rules</a>
      <a href="#">Health Check</a>
      <a href="#">VPN</a>
      <a href="#">Logs</a>
      <a href="#">Blocked Threats</a>
      <a href="#">Alerts</a>
      <a href="#">Settings</a>
    </div>
  </div>
  <div class="main">
    <div class="title-row">
      <div>
        <div class="muted">Observed network threats</div>
        <h2 style="margin:4px 0 0 0;">Dashboard</h2>
      </div>
      <div class="chip" id="healthChip">checking...</div>
    </div>
    <div class="banner" id="authBanner" style="display:none;"><span class="dot"></span><span>Local-only mode (no auth). Set AEGIS_UI_TOKEN to require Bearer auth.</span></div>

    <div class="grid">
      <div class="card"><div class="muted">Response Time</div><div class="kpi" id="respTime">—</div></div>
      <div class="card"><div class="muted">Open Ports</div><div class="kpi" id="openPorts">—</div></div>
      <div class="card"><div class="muted">Blocked Threats</div><div class="kpi" id="blocked">0</div></div>
      <div class="card"><div class="muted">Alerts</div><div class="kpi" id="alerts">0</div></div>
      <div class="card"><div class="muted">CPU</div><div class="kpi">5.7%</div></div>
      <div class="card"><div class="muted">Memory</div><div class="kpi">42%</div></div>
    </div>

    <div class="row">
      <div class="card half">
        <div class="title-row">
          <div>
            <div class="muted">Blocked Threats</div>
            <h3 style="margin:2px 0 0 0;">Live view</h3>
          </div>
          <a class="link" href="#">View all</a>
        </div>
        <table id="threatTable">
          <thead><tr><th>Severity</th><th>Rule</th><th>Source</th><th>Time</th></tr></thead>
          <tbody></tbody>
        </table>
      </div>
      <div class="card half">
        <div class="title-row">
          <div><div class="muted">Audit log</div><h3 style="margin:2px 0 0 0;">Recent events</h3></div>
          <div style="display:flex; gap:10px;">
            <a class="link" href="/v1/aegis/export">Audit export</a>
            <a class="link" href="/api/v1/support/bundle">Support bundle</a>
          </div>
        </div>
        <div id="audit" class="muted" style="max-height:240px; overflow:auto; font-family:monospace; font-size:12px;"></div>
      </div>
    </div>
  </div>
<script>
async function fetchJson(url){
  try{ const r = await fetch(url); if(!r.ok) throw new Error(r.status); return await r.json(); }
  catch(e){ console.warn("fetch fail", url, e); return null; }
}
function sevBadge(sev){
  const cls = sev === "critical" ? "critical" : sev === "high" ? "high" : "medium";
  return `<span class="sev ${cls}">${sev}</span>`;
}
async function loadStatus(){
  const h = await fetchJson("/api/v1/health");
  if(h && h.ok){ const chip=document.getElementById("healthChip"); chip.textContent="Healthy"; chip.style.background="rgba(66,211,146,.12)"; }
  const banner = document.getElementById("authBanner");
  if(banner){
    const tok = await fetchJson("/api/v1/status");
    if(tok && tok.auth && tok.auth == "open"){
      banner.style.display = "block";
    } else {
      banner.style.display = "none";
    }
  }
  const s = await fetchJson("/api/v1/status");
  if(s){
    document.getElementById("blocked").textContent = s.blockedThreats ?? 0;
    document.getElementById("alerts").textContent = s.alerts ?? 0;
    document.getElementById("respTime").textContent = "28ms";
    document.getElementById("openPorts").textContent = "32";
  }
}
async function loadThreats(){
  const data = await fetchJson("/api/v1/threats?limit=5");
  const tbody = document.querySelector("#threatTable tbody");
  if(!tbody) return;
  tbody.innerHTML = "";
  if(data && Array.isArray(data)){
    data.forEach(t=>{
      const tr = document.createElement("tr");
      tr.innerHTML = `<td>${sevBadge(t.severity)}</td><td>${t.rule}</td><td>${t.src_ip}</td><td>${t.ts}</td>`;
      tbody.appendChild(tr);
    });
  }
}
async function loadAudit(){
  const data = await fetchJson("/api/v1/audit?limit=12");
  const box = document.getElementById("audit");
  if(!box) return;
  box.innerHTML = "";
  if(data && data.lines){
    data.lines.forEach(l=>{
      const div = document.createElement("div");
      div.textContent = l;
      box.appendChild(div);
    });
  }
}
loadStatus(); loadThreats(); loadAudit();
</script>
</body>
</html>"##,
    )
}

pub async fn version() -> impl IntoResponse {
    Json(json!({
        "name": "aegis-ultra",
        "release": "dominance-v1",
        "status": "ok"
    }))
}
