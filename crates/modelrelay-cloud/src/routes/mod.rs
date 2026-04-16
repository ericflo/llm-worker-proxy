use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::sync::RwLock;

use axum::extract::Request;
use axum::middleware::Next;

use crate::state::CloudState;

mod auth;
mod checkout;
pub mod csrf;
mod dashboard;
mod legal;
mod pricing;
mod updater;
mod webhook;

static LANDING_HTML: &str = include_str!("../../templates/index.html");

/// Build the full cloud router: commercial routes + OSS admin routes.
#[must_use = "returns the configured router"]
pub fn router(state: Arc<CloudState>) -> Router {
    Router::new()
        // Commercial routes: landing page, auth, billing, pricing
        .route("/", get(landing))
        .route("/download", get(download_redirect))
        .route(
            "/download/desktop/{platform}",
            get(desktop_download_redirect),
        )
        .route(
            "/updater/desktop/{target}/{arch}/{current_version}",
            get(updater::desktop_update_check),
        )
        .route("/health", get(health))
        .route("/pricing", get(pricing::page))
        .route("/privacy", get(legal::privacy_page))
        .route("/signup", get(auth::signup_page).post(auth::signup_submit))
        .route("/terms", get(legal::terms_page))
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
        .route("/favicon-32.png", get(favicon_32_png))
        .route("/apple-touch-icon.png", get(apple_touch_icon))
        .route("/icon-512.png", get(icon_512_png))
        .route("/og-image.png", get(og_image))
        .fallback(not_found)
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(csrf::csrf_middleware))
        .layer(middleware::from_fn(session_guard))
        .with_state(state)
}

async fn landing() -> Html<&'static str> {
    Html(LANDING_HTML)
}

/// Redirect `/download` to the `#download` section on the landing page.
///
/// This exists so shareable links like `https://modelrelay.io/download`
/// resolve instead of 404ing. Uses a temporary (302) redirect so the
/// destination can be changed later without cache headaches.
async fn download_redirect() -> Redirect {
    Redirect::temporary("/#download")
}

async fn not_found(request: Request) -> impl IntoResponse {
    let path = request.uri().path().to_owned();
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
            &path,
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
  <url><loc>https://modelrelay.io/setup</loc></url>
  <url><loc>https://modelrelay.io/integrate</loc></url>
  <url><loc>https://modelrelay.io/terms</loc></url>
  <url><loc>https://modelrelay.io/privacy</loc></url>
</urlset>
"#,
    )
}

/// Real multi-size Windows .ico (16x16 + 32x32 PNG sub-images) shipped by the
/// desktop crate. Baked in so the site and desktop app share branding.
static FAVICON_ICO_BYTES: &[u8] = include_bytes!("../../../modelrelay-desktop/icons/icon.ico");
static FAVICON_32_PNG_BYTES: &[u8] = include_bytes!("../../../modelrelay-desktop/icons/32x32.png");
static APPLE_TOUCH_ICON_BYTES: &[u8] =
    include_bytes!("../../../modelrelay-desktop/icons/128x128@2x.png");
static ICON_512_PNG_BYTES: &[u8] = include_bytes!("../../../modelrelay-desktop/icons/icon.png");

async fn favicon_ico() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/x-icon"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, immutable",
            ),
        ],
        FAVICON_ICO_BYTES,
    )
}

async fn favicon_32_png() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, immutable",
            ),
        ],
        FAVICON_32_PNG_BYTES,
    )
}

async fn apple_touch_icon() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, immutable",
            ),
        ],
        APPLE_TOUCH_ICON_BYTES,
    )
}

async fn icon_512_png() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, immutable",
            ),
        ],
        ICON_512_PNG_BYTES,
    )
}

/// 1200x630 branded social-share card used by `og:image` / `twitter:image`.
///
/// Baked into the binary so it ships with the image everywhere the cloud
/// service runs and so the site's CSP (`img-src 'self' data:`) can serve it
/// same-origin without special-casing an external host.
static OG_IMAGE_BYTES: &[u8] = include_bytes!("../../assets/og-image.png");

async fn og_image() -> impl IntoResponse {
    (
        [
            (axum::http::header::CONTENT_TYPE, "image/png"),
            (
                axum::http::header::CACHE_CONTROL,
                "public, max-age=86400, immutable",
            ),
        ],
        OG_IMAGE_BYTES,
    )
}

/// Routes that do not require a session.
const SESSION_EXEMPT_ROUTES: &[&str] = &[
    "/",
    "/download",
    "/health",
    "/robots.txt",
    "/sitemap.xml",
    "/favicon.ico",
    "/favicon-32.png",
    "/apple-touch-icon.png",
    "/icon-512.png",
    "/og-image.png",
    "/checkout/cancel",
    "/terms",
    "/privacy",
];

/// URL prefixes that do not require a session. Use for routes with path
/// parameters (e.g. `/download/desktop/{platform}`).
const SESSION_EXEMPT_PREFIXES: &[&str] = &["/download/desktop/", "/updater/", "/webhook/"];

/// Middleware that returns a styled 503 page when a session-dependent route is
/// hit but the `SessionManagerLayer` has not injected a `Session` into the
/// request extensions (e.g. because the database is unreachable at startup).
async fn session_guard(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();

    // Let exempt routes through without checking for a session.
    let exempt = SESSION_EXEMPT_ROUTES.contains(&path.as_str())
        || SESSION_EXEMPT_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix));

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
                    &path,
                    body,
                    false,
                )),
            )
                .into_response();
        }
    }

    next.run(request).await
}

/// Middleware that sets standard HTTP security headers on every response.
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert(
        "referrer-policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
    headers.insert(
        "permissions-policy",
        "camera=(), microphone=(), geolocation=()".parse().unwrap(),
    );
    headers.insert(
        "content-security-policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline' https://js.stripe.com; frame-src https://js.stripe.com; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' https://api.stripe.com; font-src 'self'"
            .parse()
            .unwrap(),
    );
    response
}

/// GitHub releases API URL used to look up desktop release assets.
const DESKTOP_RELEASES_URL: &str = "https://api.github.com/repos/ericflo/modelrelay/releases";

/// Fallback redirect when the GitHub API is unreachable or no asset matches.
const DESKTOP_FALLBACK_URL: &str = "https://github.com/ericflo/modelrelay/releases/latest";

/// How long to cache the GitHub release lookup before refetching.
const DESKTOP_CACHE_TTL: Duration = Duration::from_secs(300);

/// Map of `platform -> browser_download_url` cached alongside its fetch time.
type DesktopAssetCache = Option<(Instant, HashMap<String, String>)>;

/// Process-wide cache for the resolved desktop asset URLs.
static DESKTOP_DOWNLOAD_CACHE: LazyLock<RwLock<DesktopAssetCache>> =
    LazyLock::new(|| RwLock::new(None));

/// Map a `/download/desktop/{platform}` slug to the asset filename suffix
/// produced by the Tauri release workflow.
fn desktop_asset_suffix(platform: &str) -> Option<&'static str> {
    match platform {
        "macos" => Some("_aarch64.dmg"),
        "macos-intel" => Some("_x64.dmg"),
        "linux" | "linux-deb" => Some("_amd64.deb"),
        "windows" => Some("_x64-setup.exe"),
        _ => None,
    }
}

/// Look up the latest desktop release on GitHub and return a map of
/// `platform -> browser_download_url`. Results are cached for
/// [`DESKTOP_CACHE_TTL`] to avoid hammering the GitHub API.
async fn latest_desktop_assets() -> Option<HashMap<String, String>> {
    if let Some((fetched_at, assets)) = DESKTOP_DOWNLOAD_CACHE.read().await.as_ref()
        && fetched_at.elapsed() < DESKTOP_CACHE_TTL
    {
        return Some(assets.clone());
    }

    let client = reqwest::Client::builder()
        .user_agent("modelrelay-cloud (+https://modelrelay.io)")
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let releases: Vec<Value> = client
        .get(DESKTOP_RELEASES_URL)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    let latest = releases.into_iter().find(|r| {
        r.get("tag_name")
            .and_then(Value::as_str)
            .is_some_and(|tag| tag.starts_with("desktop-v"))
    })?;

    let assets = latest.get("assets").and_then(Value::as_array)?;
    let mut map: HashMap<String, String> = HashMap::new();
    for platform in ["macos", "macos-intel", "linux", "linux-deb", "windows"] {
        let suffix = desktop_asset_suffix(platform).unwrap_or("");
        if let Some(url) = assets.iter().find_map(|a| {
            let name = a.get("name").and_then(Value::as_str)?;
            let url = a.get("browser_download_url").and_then(Value::as_str)?;
            name.ends_with(suffix).then(|| url.to_owned())
        }) {
            map.insert(platform.to_owned(), url);
        }
    }

    if map.is_empty() {
        return None;
    }

    *DESKTOP_DOWNLOAD_CACHE.write().await = Some((Instant::now(), map.clone()));
    Some(map)
}

/// Redirect `/download/desktop/{platform}` to the matching asset on the latest
/// `desktop-v*` GitHub release, with a 5-minute cache and a graceful fallback
/// to the releases landing page.
async fn desktop_download_redirect(Path(platform): Path<String>) -> Redirect {
    if desktop_asset_suffix(&platform).is_none() {
        return Redirect::temporary(DESKTOP_FALLBACK_URL);
    }

    match latest_desktop_assets()
        .await
        .and_then(|m| m.get(&platform).cloned())
    {
        Some(url) => Redirect::temporary(&url),
        None => Redirect::temporary(DESKTOP_FALLBACK_URL),
    }
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

#[cfg(test)]
mod tests {
    use super::desktop_asset_suffix;

    #[test]
    fn maps_known_platforms_to_asset_suffixes() {
        assert_eq!(desktop_asset_suffix("macos"), Some("_aarch64.dmg"));
        assert_eq!(desktop_asset_suffix("macos-intel"), Some("_x64.dmg"));
        assert_eq!(desktop_asset_suffix("linux"), Some("_amd64.deb"));
        assert_eq!(desktop_asset_suffix("linux-deb"), Some("_amd64.deb"));
        assert_eq!(desktop_asset_suffix("windows"), Some("_x64-setup.exe"));
    }

    #[test]
    fn unknown_platforms_have_no_mapping() {
        assert_eq!(desktop_asset_suffix(""), None);
        assert_eq!(desktop_asset_suffix("freebsd"), None);
        assert_eq!(desktop_asset_suffix("MACOS"), None);
    }
}
