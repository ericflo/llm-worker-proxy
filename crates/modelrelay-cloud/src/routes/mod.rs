use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use axum::extract::Request;
use axum::middleware::Next;

use crate::state::CloudState;

mod auth;
mod checkout;
pub mod csrf;
mod dashboard;
mod pricing;
mod webhook;

static LANDING_HTML: &str = include_str!("../../templates/index.html");

/// Build the full cloud router: commercial routes + OSS admin routes.
#[must_use = "returns the configured router"]
pub fn router(state: Arc<CloudState>) -> Router {
    Router::new()
        // Commercial routes: landing page, auth, billing, pricing
        .route("/", get(landing))
        .route("/health", get(health))
        .route("/pricing", get(pricing::page))
        .route("/signup", get(auth::signup_page).post(auth::signup_submit))
        .route("/login", get(auth::login_page).post(auth::login_submit))
        .route("/logout", post(auth::logout))
        .route("/checkout", post(checkout::create))
        .route("/checkout/success", get(checkout::success))
        .route("/checkout/cancel", get(checkout::cancel))
        .route("/dashboard", get(dashboard::page))
        .route("/dashboard/billing-portal", post(dashboard::billing_portal))
        .route("/dashboard/keys/generate", post(dashboard::keys_generate))
        .route("/dashboard/keys/{id}/revoke", post(dashboard::keys_revoke))
        .route("/dashboard/workers", get(dashboard::workers))
        .route("/dashboard/stats", get(dashboard::stats))
        .route("/setup", get(dashboard::setup))
        .route("/integrate", get(dashboard::integrate))
        .route("/webhook/stripe", post(webhook::handle))
        .route("/robots.txt", get(robots_txt))
        .route("/sitemap.xml", get(sitemap_xml))
        .route("/favicon.ico", get(favicon_ico))
        .fallback(not_found)
        .layer(middleware::from_fn(csrf::csrf_middleware))
        .layer(middleware::from_fn(session_guard))
        .with_state(state)
}

async fn landing() -> Html<&'static str> {
    Html(LANDING_HTML)
}

async fn not_found() -> impl IntoResponse {
    let body = r#"<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;min-height:60vh;text-align:center;padding:2rem">
<h1 style="font-size:6rem;font-weight:800;color:#7c3aed;line-height:1;margin:0">404</h1>
<h2 style="font-size:1.5rem;font-weight:600;margin:1rem 0 .5rem">Page Not Found</h2>
<p style="color:#8b949e;max-width:28rem;margin-bottom:2rem">The page you're looking for doesn't exist or has been moved.</p>
<a href="/" style="display:inline-block;padding:.75rem 2rem;background:#7c3aed;color:#fff;text-decoration:none;border-radius:.5rem;font-weight:600;transition:background .2s">Back to Home</a>
</div>"#;
    (
        StatusCode::NOT_FOUND,
        Html(modelrelay_web::templates::page_shell(
            "404 — Not Found",
            body,
            false,
        )),
    )
}

async fn robots_txt() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        "User-agent: *\nAllow: /\nSitemap: https://modelrelay.io/sitemap.xml\n",
    )
}

async fn sitemap_xml() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/xml; charset=utf-8",
        )],
        r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://modelrelay.io/</loc></url>
  <url><loc>https://modelrelay.io/pricing</loc></url>
  <url><loc>https://modelrelay.io/signup</loc></url>
  <url><loc>https://modelrelay.io/login</loc></url>
</urlset>
"#,
    )
}

async fn favicon_ico() -> impl IntoResponse {
    // Minimal SVG favicon matching the purple "M" logo used in the HTML head.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="6" fill="#7c3aed"/><text x="16" y="24" font-size="22" font-weight="bold" fill="white" text-anchor="middle" font-family="system-ui,sans-serif">M</text></svg>"##;
    ([(axum::http::header::CONTENT_TYPE, "image/svg+xml")], svg)
}

/// Routes that do not require a session.
const SESSION_EXEMPT_ROUTES: &[&str] = &[
    "/",
    "/health",
    "/robots.txt",
    "/sitemap.xml",
    "/favicon.ico",
    "/checkout/cancel",
];

/// Middleware that returns a styled 503 page when a session-dependent route is
/// hit but the `SessionManagerLayer` has not injected a `Session` into the
/// request extensions (e.g. because the database is unreachable at startup).
async fn session_guard(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();

    // Let exempt routes through without checking for a session.
    let exempt = SESSION_EXEMPT_ROUTES.contains(&path.as_str()) || path.starts_with("/webhook/");

    if !exempt {
        let has_session = request
            .extensions()
            .get::<tower_sessions::Session>()
            .is_some();

        if !has_session {
            let body = r#"<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;min-height:60vh;text-align:center;padding:2rem">
<h1 style="font-size:3rem;font-weight:800;color:#fbbf24;line-height:1;margin:0">503</h1>
<h2 style="font-size:1.5rem;font-weight:600;margin:1rem 0 .5rem">Service Temporarily Unavailable</h2>
<p style="color:#8b949e;max-width:28rem;margin-bottom:2rem">We're experiencing a temporary issue connecting to our database. Please try again in a few moments.</p>
<a href="/" style="display:inline-block;padding:.75rem 2rem;background:#7c3aed;color:#fff;text-decoration:none;border-radius:.5rem;font-weight:600;transition:background .2s">Back to Home</a>
</div>"#;
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Html(modelrelay_web::templates::page_shell(
                    "Service Unavailable",
                    body,
                    false,
                )),
            )
                .into_response();
        }
    }

    next.run(request).await
}

async fn health(State(state): State<Arc<CloudState>>) -> (StatusCode, Json<Value>) {
    let db_ok = if let Some(ref pool) = state.db {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(pool)
            .await
            .is_ok()
    } else {
        false
    };

    let (status_code, status_text) = if db_ok {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "degraded")
    };

    (
        status_code,
        Json(json!({
            "status": status_text,
            "db_connected": db_ok,
            "stripe_configured": state.stripe_key.is_some(),
        })),
    )
}
