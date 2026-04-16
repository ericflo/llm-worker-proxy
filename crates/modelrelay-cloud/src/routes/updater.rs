//! Tauri v2 auto-updater endpoint.
//!
//! The desktop app is configured to hit
//! `https://modelrelay.io/updater/desktop/{target}/{arch}/{current_version}`
//! on launch and when the user clicks "Check for Updates". This module serves
//! the JSON payload Tauri's updater plugin expects, or returns 204 No Content
//! when the caller is already on the latest version.
//!
//! The GitHub release lookup and per-release asset list are cached for five
//! minutes (shared with the `/download/desktop/{platform}` redirect), so we
//! won't hammer the GitHub API even if every desktop instance checks in at
//! roughly the same time after a release lands.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, http::header};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::RwLock;

/// GitHub releases API URL used to look up desktop release assets.
const RELEASES_URL: &str = "https://api.github.com/repos/ericflo/modelrelay/releases";

/// How long to cache the GitHub release lookup before refetching.
const CACHE_TTL: Duration = Duration::from_secs(300);

/// Cached latest release info. The assets vector is the raw GitHub API asset
/// list for the most recent `desktop-v*` release.
#[derive(Clone)]
struct LatestRelease {
    version: String,
    published_at: Option<String>,
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Clone)]
struct Asset {
    name: String,
    url: String,
}

static LATEST_CACHE: LazyLock<RwLock<Option<(Instant, LatestRelease)>>> =
    LazyLock::new(|| RwLock::new(None));

/// Tauri v2 dynamic updater JSON response. Matches the shape documented at
/// <https://v2.tauri.app/plugin/updater/>.
#[derive(Serialize)]
struct UpdateManifest {
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub_date: Option<String>,
    url: String,
    signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

/// Public entry point: returns the latest release (cached), or `None` if the
/// GitHub API can't be reached or no `desktop-v*` release exists.
async fn latest_release() -> Option<LatestRelease> {
    if let Some((fetched_at, release)) = LATEST_CACHE.read().await.as_ref()
        && fetched_at.elapsed() < CACHE_TTL
    {
        return Some(release.clone());
    }

    let client = reqwest::Client::builder()
        .user_agent("modelrelay-cloud (+https://modelrelay.io)")
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let releases: Vec<Value> = client
        .get(RELEASES_URL)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;

    let latest_value = releases.into_iter().find(|r| {
        r.get("tag_name")
            .and_then(Value::as_str)
            .is_some_and(|tag| tag.starts_with("desktop-v"))
    })?;

    let tag = latest_value.get("tag_name").and_then(Value::as_str)?;
    let version = tag.strip_prefix("desktop-v").unwrap_or(tag).to_owned();
    let published_at = latest_value
        .get("published_at")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let body = latest_value
        .get("body")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let assets = latest_value
        .get("assets")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|a| {
            Some(Asset {
                name: a.get("name").and_then(Value::as_str)?.to_owned(),
                url: a
                    .get("browser_download_url")
                    .and_then(Value::as_str)?
                    .to_owned(),
            })
        })
        .collect();

    let release = LatestRelease {
        version,
        published_at,
        body,
        assets,
    };

    *LATEST_CACHE.write().await = Some((Instant::now(), release.clone()));
    Some(release)
}

/// Decide whether the server version strictly beats the client version.
///
/// We do a best-effort semver compare on the first three dot-separated numeric
/// components. Any parse failure falls back to a lexicographic comparison so a
/// malformed tag never locks clients out of updates.
fn is_newer(server: &str, client: &str) -> bool {
    fn parse3(v: &str) -> Option<(u32, u32, u32)> {
        // Strip a leading `v` if present.
        let v = v.strip_prefix('v').unwrap_or(v);
        // Drop any pre-release / build-metadata suffix so "0.1.1-alpha.2"
        // parses as (0, 1, 1).
        let core = v.split(['-', '+']).next().unwrap_or(v);
        let mut it = core.split('.');
        let a = it.next()?.parse().ok()?;
        let b = it.next()?.parse().ok()?;
        let c = it.next()?.parse().ok()?;
        Some((a, b, c))
    }
    match (parse3(server), parse3(client)) {
        (Some(s), Some(c)) => s > c,
        _ => server > client,
    }
}

/// For a given Tauri `{target}/{arch}` pair, return the filename suffix of the
/// updater-signed artifact we expect the release workflow to upload. If
/// `createUpdaterArtifacts: true` is set in `tauri.conf.json`, the Tauri v2
/// bundler produces:
///
/// | Platform | Artifact served as `url`       | Sig file               |
/// |----------|--------------------------------|------------------------|
/// | macOS    | `*.app.tar.gz`                 | `*.app.tar.gz.sig`     |
/// | Linux    | `*.AppImage`                   | `*.AppImage.sig`       |
/// | Windows  | `*-setup.exe` (NSIS installer) | `*-setup.exe.sig`      |
///
/// We match by suffix plus an architecture hint so a future universal or
/// multi-arch release doesn't cross-serve the wrong binary.
fn artifact_suffixes(target: &str, arch: &str) -> Option<&'static [&'static str]> {
    match (target, arch) {
        // macOS: Tauri's app bundle tarball. The arch appears as either
        // `aarch64` or `x64` in the filename (Tauri keeps historical `x64`
        // for Intel even though the target triple is `x86_64`).
        ("darwin", "aarch64") => Some(&["_aarch64.app.tar.gz", "-aarch64.app.tar.gz"]),
        ("darwin", "x86_64") => Some(&["_x64.app.tar.gz", "-x64.app.tar.gz"]),
        // Linux: AppImage is the canonical auto-updatable artifact.
        ("linux", "x86_64") => Some(&["_amd64.AppImage", "-amd64.AppImage", "_x86_64.AppImage"]),
        // Windows: the NSIS installer is what `createUpdaterArtifacts` signs.
        ("windows", "x86_64") => Some(&["_x64-setup.exe", "-x64-setup.exe"]),
        _ => None,
    }
}

/// Find the signed bundle URL and the matching `.sig` URL for a given target.
fn find_pair(assets: &[Asset], suffixes: &[&str]) -> Option<(String, String)> {
    let bundle = assets
        .iter()
        .find(|a| suffixes.iter().any(|s| a.name.ends_with(s)))?;
    let sig_name = format!("{}.sig", bundle.name);
    let sig = assets.iter().find(|a| a.name == sig_name)?;
    Some((bundle.url.clone(), sig.url.clone()))
}

/// Download the raw base64 contents of a `.sig` file from a GitHub release
/// asset URL. Tauri expects this exact string inlined into the JSON.
async fn fetch_signature(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .user_agent("modelrelay-cloud (+https://modelrelay.io)")
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let resp = client
        .get(url)
        .header(header::ACCEPT, "application/octet-stream")
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let text = resp.text().await.ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Signature cache keyed by the asset's download URL. Signatures don't change
/// for a released artifact, so we can cache them indefinitely for the lifetime
/// of the process.
static SIGNATURE_CACHE: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

async fn fetch_signature_cached(url: &str) -> Option<String> {
    if let Some(sig) = SIGNATURE_CACHE.read().await.get(url) {
        return Some(sig.clone());
    }
    let sig = fetch_signature(url).await?;
    SIGNATURE_CACHE
        .write()
        .await
        .insert(url.to_owned(), sig.clone());
    Some(sig)
}

/// Route handler: `GET /updater/desktop/{target}/{arch}/{current_version}`.
pub async fn desktop_update_check(
    Path((target, arch, current_version)): Path<(String, String, String)>,
) -> Response {
    let Some(suffixes) = artifact_suffixes(&target, &arch) else {
        // Unknown platform — tell the client to look elsewhere.
        return StatusCode::NO_CONTENT.into_response();
    };

    let Some(release) = latest_release().await else {
        // Can't reach GitHub — treat as "no update available". The client
        // will retry on the next check.
        return StatusCode::NO_CONTENT.into_response();
    };

    if !is_newer(&release.version, &current_version) {
        return StatusCode::NO_CONTENT.into_response();
    }

    let Some((bundle_url, sig_url)) = find_pair(&release.assets, suffixes) else {
        // Release exists but no matching artifact for this platform — don't
        // error, just tell the client there's nothing to install.
        return StatusCode::NO_CONTENT.into_response();
    };

    let Some(signature) = fetch_signature_cached(&sig_url).await else {
        return StatusCode::NO_CONTENT.into_response();
    };

    let manifest = UpdateManifest {
        version: release.version,
        pub_date: release.published_at,
        url: bundle_url,
        signature,
        notes: release.body,
    };

    (
        StatusCode::OK,
        [(
            header::CACHE_CONTROL,
            "public, max-age=60, stale-while-revalidate=300",
        )],
        Json(manifest),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{Asset, artifact_suffixes, find_pair, is_newer};

    #[test]
    fn newer_semver_wins() {
        assert!(is_newer("0.1.2", "0.1.1"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.1", "0.1.1"));
        assert!(!is_newer("0.1.0", "0.1.1"));
    }

    #[test]
    fn prerelease_suffixes_are_ignored() {
        // Versions with suffixes still compare by their semver core.
        assert!(!is_newer("0.1.1", "0.1.1-alpha.1"));
        assert!(is_newer("0.1.2", "0.1.1-rc.5"));
    }

    #[test]
    fn malformed_versions_fall_back_to_string_compare() {
        assert!(!is_newer("abc", "abc"));
        assert!(is_newer("z", "a"));
    }

    #[test]
    fn platform_suffixes_cover_expected_targets() {
        assert!(artifact_suffixes("darwin", "aarch64").is_some());
        assert!(artifact_suffixes("darwin", "x86_64").is_some());
        assert!(artifact_suffixes("linux", "x86_64").is_some());
        assert!(artifact_suffixes("windows", "x86_64").is_some());
        assert!(artifact_suffixes("linux", "aarch64").is_none());
        assert!(artifact_suffixes("bogus", "x86_64").is_none());
    }

    fn a(name: &str) -> Asset {
        Asset {
            name: name.to_owned(),
            url: format!("https://example.com/{name}"),
        }
    }

    #[test]
    fn find_pair_matches_bundle_and_sig() {
        let assets = vec![
            a("ModelRelay_0.1.2_aarch64.app.tar.gz"),
            a("ModelRelay_0.1.2_aarch64.app.tar.gz.sig"),
            a("ModelRelay_0.1.2_x64-setup.exe"),
            a("ModelRelay_0.1.2_x64-setup.exe.sig"),
        ];
        let suffixes = artifact_suffixes("darwin", "aarch64").unwrap();
        let (bundle, sig) = find_pair(&assets, suffixes).expect("match");
        assert!(bundle.ends_with("_aarch64.app.tar.gz"));
        assert!(sig.ends_with("_aarch64.app.tar.gz.sig"));
    }

    #[test]
    fn find_pair_requires_a_matching_sig() {
        // Missing .sig → no match, so the endpoint returns 204 instead of
        // serving a bundle the client can't verify.
        let assets = vec![a("ModelRelay_0.1.2_aarch64.app.tar.gz")];
        let suffixes = artifact_suffixes("darwin", "aarch64").unwrap();
        assert!(find_pair(&assets, suffixes).is_none());
    }
}
