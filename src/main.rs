mod config;
mod audit;
mod dlp;
mod opa;
mod approvals;
mod bundle;
mod gateway;
mod tools;

use axum::{routing::{get, post}, Router};
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() {
    let cfg = config::Config::load().expect("config load failed");
    let state = cfg.build_state().await.expect("state init failed");

    let cors = CorsLayer::new().allow_origin(Any).allow_headers(Any).allow_methods(Any);

    let app = Router::new()
        .route("/healthz", get(gateway::healthz))
        .route("/v1/chat/completions", post(gateway::chat_completions))
        .route("/v1/tools/prepare", post(tools::prepare))
        .route("/v1/tools/commit", post(tools::commit))
        .route("/v1/aegis/export", get(gateway::export_audit))
        .route("/v1/aegis/bundle/:request_id", get(bundle::get_bundle))
        .route("/v1/approvals/sign", post(approvals::sign_dev_approval))
        .layer(cors)
        .with_state(state);

    let addr = cfg.bind_addr();
    println!("Aegis Ultra listening on http://{}", addr);

    axum::serve(tokio::net::TcpListener::bind(addr).await.unwrap(), app)
        .await
        .unwrap();
}