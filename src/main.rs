mod approvals;
mod audit;
mod bundle;
mod config;
mod decision;
mod dlp;
mod gateway;
mod opa;
mod tools;
mod ui;

use axum::http::{HeaderValue, Request};
use axum::{
    body::Body,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use dashmap::DashMap;
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

async fn auth_middleware(token: Option<String>, req: Request<Body>, next: Next) -> Response {
    // allow-list public routes
    let path = req.uri().path();
    let public = matches!(
        path,
        "/" | "/dashboard"
            | "/healthz"
            | "/readyz"
            | "/version"
            | "/api/v1/health"
            | "/api/v1/status"
    );
    if public || token.is_none() {
        return next.run(req).await;
    }
    if let Some(h) = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
    {
        if h == format!("Bearer {}", token.clone().unwrap()) {
            return next.run(req).await;
        }
    }
    Response::builder()
        .status(axum::http::StatusCode::UNAUTHORIZED)
        .body(axum::body::Body::from("unauthorized"))
        .unwrap()
}

#[derive(Clone, Copy)]
struct Bucket {
    tokens: f64,
    last: Instant,
    last_seen: Instant,
}

#[derive(Clone)]
struct RateLimiter {
    inner: Arc<DashMap<IpAddr, Bucket>>,
    rate_per_sec: f64,
    burst: f64,
}

impl RateLimiter {
    fn new(rate_per_sec: f64, burst: f64) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            rate_per_sec,
            burst,
        }
    }
    fn allow(&self, ip: IpAddr, cost: f64) -> (bool, Duration) {
        let now = Instant::now();
        let mut entry = self.inner.entry(ip).or_insert(Bucket {
            tokens: self.burst,
            last: now,
            last_seen: now,
        });
        let elapsed = now.duration_since(entry.last).as_secs_f64();
        if elapsed > 0.0 {
            entry.tokens = (entry.tokens + elapsed * self.rate_per_sec).min(self.burst);
            entry.last = now;
        }
        entry.last_seen = now;
        if entry.tokens >= cost {
            entry.tokens -= cost;
            (true, Duration::from_millis(0))
        } else {
            let deficit = cost - entry.tokens;
            let secs = deficit / self.rate_per_sec.max(1e-6);
            (false, Duration::from_secs_f64(secs))
        }
    }
    fn _cleanup(&self, ttl: Duration) {
        let now = Instant::now();
        self.inner
            .retain(|_, b| now.duration_since(b.last_seen) <= ttl);
    }
}

fn client_ip<B>(req: &Request<B>) -> Option<IpAddr> {
    req.extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip())
}

#[derive(Clone)]
struct RequestId(pub String);

async fn request_id_middleware(req: Request<Body>, next: Next) -> Response {
    let mut req = req;
    let rid = Uuid::new_v4().to_string();
    req.extensions_mut().insert(RequestId(rid.clone()));
    let mut res = next.run(req).await;
    res.headers_mut()
        .insert("x-request-id", HeaderValue::from_str(&rid).unwrap());
    res
}

async fn rate_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<RateLimiter>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, axum::http::StatusCode> {
    if let Some(ip) = client_ip(&req) {
        let path = req.uri().path();
        let cost = if path.starts_with("/api/v1/support/bundle") {
            5.0
        } else {
            1.0
        };
        let (ok, retry_after) = limiter.allow(ip, cost);
        if !ok {
            let mut resp = Response::new(axum::body::Body::from("rate limit exceeded"));
            *resp.status_mut() = axum::http::StatusCode::TOO_MANY_REQUESTS;
            resp.headers_mut().insert(
                "Retry-After",
                HeaderValue::from_str(&retry_after.as_secs().max(1).to_string()).unwrap(),
            );
            return Ok(resp);
        }
    }
    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() {
    let cfg = config::Config::load().expect("config load failed");
    let state = cfg.build_state().await.expect("state init failed");

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_headers(Any)
        .allow_methods(Any);

    let auth_token = state.auth_token.clone();
    let limiter = RateLimiter::new(30.0, 60.0);

    let app = Router::new()
        .route("/", get(ui::index))
        .route("/dashboard", get(ui::index))
        .route("/version", get(ui::version))
        .route("/healthz", get(gateway::healthz))
        .route("/readyz", get(gateway::readyz))
        .route("/api/v1/health", get(gateway::api_health))
        .route("/api/v1/status", get(gateway::api_status))
        .route("/api/v1/threats", get(gateway::api_threats))
        .route("/api/v1/threats/summary", get(gateway::api_threats_summary))
        .route("/api/v1/audit", get(gateway::api_audit))
        .route("/api/v1/support/bundle", get(gateway::support_bundle))
        .route("/v1/chat/completions", post(gateway::chat_completions))
        .route("/v1/tools/prepare", post(tools::prepare))
        .route("/v1/tools/commit", post(tools::commit))
        .route("/v1/aegis/export", get(gateway::export_audit))
        .route("/v1/aegis/bundle/:request_id", get(bundle::get_bundle))
        .route("/v1/approvals/sign", post(approvals::sign_dev_approval))
        .layer(cors)
        .with_state(state)
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn_with_state(
            limiter.clone(),
            rate_limit_middleware,
        ));

    let auth = middleware::from_fn({
        let tok = auth_token.clone();
        move |req, next| {
            let t = tok.clone();
            async move { auth_middleware(t.clone(), req, next).await }
        }
    });
    let sec = middleware::map_response(|mut res: Response| async move {
        let headers = res.headers_mut();
        headers.insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
        headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
        headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
        headers.insert("content-security-policy", HeaderValue::from_static("default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'"));
        headers.insert(
            "permissions-policy",
            HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
        );
        headers.insert(
            "cross-origin-resource-policy",
            HeaderValue::from_static("same-origin"),
        );
        headers.insert(
            "cross-origin-opener-policy",
            HeaderValue::from_static("same-origin"),
        );
        headers.insert("cache-control", HeaderValue::from_static("no-store"));
        headers.insert("pragma", HeaderValue::from_static("no-cache"));
        res
    });
    let app = app.layer(auth).layer(sec);

    let addr = cfg.bind_addr();
    println!("Aegis Ultra listening on http://{}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
