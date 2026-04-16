use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use tower_sessions::{MemoryStore, SessionManagerLayer};

use modelrelay_cloud::routes;
use modelrelay_cloud::state::CloudState;

/// Build the cloud router with no database and no Stripe — the minimum viable
/// state for smoke-testing routes that don't require either.
///
/// NOTE: These tests run without a session layer, so the CSRF middleware
/// passes through (no session in request extensions = skip validation).
fn test_state() -> Arc<CloudState> {
    Arc::new(CloudState {
        db: None,
        stripe_key: None,
        webhook_secret: Some("whsec_test_secret".into()),
        admin_url: None,
        admin_token: None,
        admin_emails: vec![],
        rate_limiter: std::sync::Arc::new(modelrelay_cloud::state::RateLimiter::new(
            5,
            std::time::Duration::from_secs(15 * 60),
        )),
    })
}

fn app() -> axum::Router {
    routes::router(test_state())
}

/// Build a router with an in-memory session layer attached, so session-dependent
/// routes don't 503 under the session guard middleware. Used by tests that
/// want to inspect rendered page HTML (canonical/og meta tags, etc.).
fn app_with_session() -> axum::Router {
    let session_layer = SessionManagerLayer::new(MemoryStore::default());
    routes::router(test_state()).layer(session_layer)
}

async fn get_with_session(path: &str) -> (StatusCode, String) {
    let resp = app_with_session()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn get(path: &str) -> (StatusCode, String) {
    let resp = app()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn post(path: &str, content_type: &str, body: &str) -> (StatusCode, String) {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", content_type)
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

// ─── Landing page ──────────────────────────────────────────────────────────

#[tokio::test]
async fn landing_page_returns_200_with_html() {
    let (status, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("ModelRelay") || body.contains("<!DOCTYPE") || body.contains("<html"),
        "expected HTML landing page, got: {}",
        &body[..body.len().min(200)]
    );
}

// ─── Health endpoint ───────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_503_when_db_unavailable() {
    let (status, body) = get("/health").await;
    // db: None → should return 503
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let json: serde_json::Value = serde_json::from_str(&body).expect("health response is JSON");
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["db_connected"], false);
    assert_eq!(json["stripe_configured"], false);
}

// ─── Pricing page ──────────────────────────────────────────────────────────

#[tokio::test]
async fn pricing_returns_503_without_session_layer() {
    let (status, body) = get("/pricing").await;
    // Without a session layer the session guard middleware returns a styled 503.
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

// ─── Auth pages ────────────────────────────────────────────────────────────

#[tokio::test]
async fn signup_page_returns_503_without_session_layer() {
    let (status, body) = get("/signup").await;
    // Without a session layer the session guard middleware returns a styled 503.
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

#[tokio::test]
async fn login_page_returns_503_without_session_layer() {
    let (status, body) = get("/login").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

// ─── POST /signup without DB returns error ─────────────────────────────────

#[tokio::test]
async fn signup_submit_without_session_returns_503() {
    let (status, body) = post(
        "/signup",
        "application/x-www-form-urlencoded",
        "email=test%40example.com&password=longpassword123",
    )
    .await;
    // Without session layer the session guard middleware returns 503.
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

// ─── POST /login with no DB returns error ──────────────────────────────────

#[tokio::test]
async fn login_submit_without_session_returns_503() {
    let (status, body) = post(
        "/login",
        "application/x-www-form-urlencoded",
        "email=test%40example.com&password=wrongpassword",
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

// ─── GET /dashboard without session redirects to /login ────────────────────

#[tokio::test]
async fn dashboard_without_session_returns_503() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Without a session layer the session guard middleware returns 503.
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ─── Checkout routes ──────────────────────────────────────────────────────

#[tokio::test]
async fn checkout_without_session_returns_503() {
    // Without a session layer the session guard middleware returns 503.
    let (status, body) = post("/checkout", "application/x-www-form-urlencoded", "").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

#[tokio::test]
async fn checkout_cancel_returns_200() {
    let (status, body) = get("/checkout/cancel").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains('<') && body.len() > 50,
        "expected HTML cancel page"
    );
}

#[tokio::test]
async fn checkout_success_without_session_returns_503() {
    // GET /checkout/success uses Session, so without session layer → 503.
    let (status, body) = get("/checkout/success").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("Service Temporarily Unavailable"),
        "expected styled 503 page"
    );
}

// ─── SEO routes ───────────────────────────────────────────────────────────

#[tokio::test]
async fn robots_txt_returns_200_with_correct_content() {
    let (status, body) = get("/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("User-agent: *"),
        "missing User-agent directive"
    );
    assert!(body.contains("Allow: /"), "missing Allow directive");
    assert!(
        body.contains("Sitemap: https://modelrelay.io/sitemap.xml"),
        "missing Sitemap directive"
    );
}

#[tokio::test]
async fn sitemap_xml_returns_200_with_correct_content() {
    let (status, body) = get("/sitemap.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("http://www.sitemaps.org/schemas/sitemap/0.9"),
        "missing sitemap namespace"
    );
    for path in &["/", "/pricing", "/signup", "/login", "/setup", "/integrate"] {
        assert!(
            body.contains(&format!("https://modelrelay.io{path}")),
            "missing URL for {path}"
        );
    }
}

#[tokio::test]
async fn download_redirects_to_hash_anchor() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/download")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TEMPORARY_REDIRECT,
        "/download should return 307 Temporary Redirect"
    );
    let location = resp
        .headers()
        .get(axum::http::header::LOCATION)
        .expect("Location header should be set")
        .to_str()
        .expect("Location header should be valid UTF-8");
    assert_eq!(
        location, "/#download",
        "/download should redirect to the #download anchor on the landing page"
    );
}

#[tokio::test]
async fn desktop_download_unknown_platform_falls_back_to_releases_page() {
    let resp = app()
        .oneshot(
            Request::builder()
                .uri("/download/desktop/freebsd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = resp
        .headers()
        .get(axum::http::header::LOCATION)
        .expect("Location header should be set")
        .to_str()
        .expect("Location header should be valid UTF-8");
    assert_eq!(
        location, "https://github.com/ericflo/modelrelay/releases/latest",
        "unknown platform should redirect to the releases landing page"
    );
}

#[tokio::test]
async fn favicon_returns_success() {
    let (status, body) = get("/favicon.ico").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<svg"), "expected SVG favicon");
}

// ─── Stripe webhook ────────────────────────────────────────────────────────

#[tokio::test]
async fn webhook_without_signature_returns_400() {
    let (status, _body) = post(
        "/webhook/stripe",
        "application/json",
        r#"{"type":"checkout.session.completed"}"#,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "webhook without Stripe-Signature should return 400"
    );
}

#[tokio::test]
async fn webhook_with_invalid_signature_returns_400() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/stripe")
                .header("content-type", "application/json")
                .header(
                    "Stripe-Signature",
                    "t=1700000000,v1=0000000000000000000000000000000000000000000000000000000000000000",
                )
                .body(Body::from(
                    r#"{"type":"checkout.session.completed"}"#.to_owned(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "webhook with invalid signature should return 400"
    );
}

#[tokio::test]
async fn webhook_with_missing_v1_returns_400() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/stripe")
                .header("content-type", "application/json")
                .header("Stripe-Signature", "t=1700000000")
                .body(Body::from("{}".to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─── Security headers ─────────────────────────────────────────────────────

#[tokio::test]
async fn responses_include_security_headers() {
    let resp = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let headers = resp.headers();
    assert_eq!(
        headers
            .get("x-content-type-options")
            .map(|v| v.to_str().unwrap()),
        Some("nosniff"),
    );
    assert_eq!(
        headers.get("x-frame-options").map(|v| v.to_str().unwrap()),
        Some("DENY"),
    );
    assert_eq!(
        headers.get("referrer-policy").map(|v| v.to_str().unwrap()),
        Some("strict-origin-when-cross-origin"),
    );
    assert_eq!(
        headers
            .get("permissions-policy")
            .map(|v| v.to_str().unwrap()),
        Some("camera=(), microphone=(), geolocation=()"),
    );
    let csp = headers
        .get("content-security-policy")
        .map(|v| v.to_str().unwrap());
    assert!(csp.is_some(), "missing content-security-policy header");
    assert!(
        csp.unwrap().contains("https://js.stripe.com"),
        "CSP must allow Stripe JS"
    );
}

// ─── Per-page og:url + canonical meta tags ────────────────────────────────

#[tokio::test]
async fn landing_page_has_root_og_url_and_canonical() {
    let (status, body) = get_with_session("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"<meta property="og:url" content="https://modelrelay.io/""#),
        "landing page missing og:url for root path"
    );
    assert!(
        body.contains(r#"<link rel="canonical" href="https://modelrelay.io/""#),
        "landing page missing canonical for root path"
    );
}

#[tokio::test]
async fn login_page_has_per_page_og_url_and_canonical() {
    let (status, body) = get_with_session("/login").await;
    assert_eq!(status, StatusCode::OK, "/login should render with session");
    assert!(
        body.contains(r#"<meta property="og:url" content="https://modelrelay.io/login""#),
        "/login should have og:url pointing at /login, got body starting: {}",
        &body[..body.len().min(500)]
    );
    assert!(
        body.contains(r#"<link rel="canonical" href="https://modelrelay.io/login""#),
        "/login should have canonical link pointing at /login"
    );
}

#[tokio::test]
async fn signup_page_has_per_page_og_url_and_canonical() {
    let (status, body) = get_with_session("/signup").await;
    assert_eq!(status, StatusCode::OK, "/signup should render with session");
    assert!(
        body.contains(r#"<meta property="og:url" content="https://modelrelay.io/signup""#),
        "/signup should have og:url pointing at /signup"
    );
    assert!(
        body.contains(r#"<link rel="canonical" href="https://modelrelay.io/signup""#),
        "/signup should have canonical link pointing at /signup"
    );
}

#[tokio::test]
async fn pricing_page_has_og_url_and_canonical() {
    let (status, body) = get_with_session("/pricing").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "/pricing should render with session"
    );
    assert!(
        body.contains(r#"<meta property="og:url" content="https://modelrelay.io/pricing""#),
        "/pricing should have og:url pointing at /pricing"
    );
    assert!(
        body.contains(r#"<link rel="canonical" href="https://modelrelay.io/pricing""#),
        "/pricing should have canonical link pointing at /pricing"
    );
    assert!(
        body.contains(r#"<meta property="og:title" content="Pricing — ModelRelay""#),
        "/pricing should have og:title"
    );
    assert!(
        body.contains(r#"<meta property="og:description""#),
        "/pricing should have og:description"
    );
    assert!(
        body.contains(r#"<meta property="og:type" content="website""#),
        "/pricing should have og:type"
    );
    assert!(
        body.contains(r#"<meta name="twitter:card" content="summary""#),
        "/pricing should have twitter:card"
    );
    assert!(
        body.contains(r#"<meta name="twitter:title" content="Pricing — ModelRelay""#),
        "/pricing should have twitter:title"
    );
    assert!(
        body.contains(r#"<meta name="twitter:description""#),
        "/pricing should have twitter:description"
    );
}

#[tokio::test]
async fn checkout_cancel_has_per_page_og_url_and_canonical() {
    // /checkout/cancel is session-exempt, so it renders even without the session layer.
    let (status, body) = get("/checkout/cancel").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"<meta property="og:url" content="https://modelrelay.io/checkout/cancel""#),
        "/checkout/cancel should have og:url pointing at /checkout/cancel"
    );
    assert!(
        body.contains(r#"<link rel="canonical" href="https://modelrelay.io/checkout/cancel""#),
        "/checkout/cancel should have canonical link pointing at /checkout/cancel"
    );
}

// ─── Webhook with no webhook_secret configured returns 500 ────────────────

#[tokio::test]
async fn webhook_without_secret_configured_returns_500() {
    let state = Arc::new(CloudState {
        db: None,
        stripe_key: None,
        webhook_secret: None, // no secret configured
        admin_url: None,
        admin_token: None,
        admin_emails: vec![],
        rate_limiter: std::sync::Arc::new(modelrelay_cloud::state::RateLimiter::new(
            5,
            std::time::Duration::from_secs(15 * 60),
        )),
    });

    let resp = routes::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/stripe")
                .header("content-type", "application/json")
                .header("Stripe-Signature", "t=1700000000,v1=abc")
                .body(Body::from("{}".to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "webhook with no secret configured should return 500"
    );
}
