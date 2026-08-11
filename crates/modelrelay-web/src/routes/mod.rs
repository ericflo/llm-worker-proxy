use axum::response::Html;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tower_http::services::ServeDir;

/// Build the OSS admin dashboard router.
///
/// This provides health checks and the admin monitoring dashboard for
/// self-hosted deployments. The commercial `modelrelay-cloud` crate adds
/// Stripe billing, user accounts, and its own routes on top.
pub fn router() -> Router {
    Router::new()
        .route("/", get(landing))
        .route("/health", get(health))
        .route("/dashboard", get(dashboard))
        .route("/setup", get(setup))
        .route("/integrate", get(integrate))
        .nest_service("/static", ServeDir::new("crates/modelrelay-web/static"))
}

async fn landing() -> Html<String> {
    Html(crate::templates::page_shell(
        "ModelRelay Admin",
        "/",
        r#"<div class="card">
           <h2>Welcome to ModelRelay</h2>
           <p>This is the open-source admin dashboard for your self-hosted ModelRelay deployment.</p>
           <div style="display:flex; gap:12px; margin-top:16px; flex-wrap:wrap;">
             <a href="/dashboard" class="btn">Dashboard</a>
             <a href="/setup" class="btn">Setup Wizard</a>
             <a href="/integrate" class="btn">Integrations</a>
           </div>
         </div>
         <div class="card">
           <h2>Quick Links</h2>
           <ul style="color:#8b949e; line-height:1.8;">
             <li><a href="https://github.com/ericflo/modelrelay" target="_blank">GitHub Repository</a></li>
             <li><a href="https://ericflo.github.io/modelrelay/" target="_blank">Documentation</a></li>
             <li><a href="https://modelrelay.io" target="_blank">Hosted Version</a></li>
           </ul>
         </div>"#,
        false,
    ))
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
    }))
}

async fn dashboard() -> Html<String> {
    Html(crate::templates::dashboard_page())
}

async fn setup() -> Html<String> {
    Html(crate::templates::setup_wizard_page())
}

async fn integrate() -> Html<String> {
    Html(crate::templates::integrate_page())
}
