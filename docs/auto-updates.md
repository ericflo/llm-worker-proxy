# Desktop Auto-Updates

The ModelRelay desktop app ships with Tauri v2's auto-updater so users never
have to manually reinstall to get new versions. This page explains how it
works end-to-end, how to cut a release that gets picked up by the updater,
and how to rotate the signing key if it's ever compromised.

---

## How It Works

```
  ┌──────────────────────┐        ┌─────────────────────────────┐        ┌──────────────────────┐
  │ ModelRelay.app (v1)  │ ─GET─▶ │ modelrelay.io/updater/...   │ ─GET─▶ │ GitHub Releases API  │
  │ tauri-plugin-updater │ ◀204── │ (Axum, in modelrelay-cloud) │ ◀JSON─ │                      │
  └──────────┬───────────┘        └─────────────────────────────┘        └──────────────────────┘
             │  200 + JSON when a newer `desktop-v*` release exists
             ▼
  ┌──────────────────────┐
  │ Download .app.tar.gz │──► Verify minisign sig (pubkey baked into app)
  │ / .AppImage / -setup │──► Unpack + replace binary
  │ .exe from GitHub     │──► Prompt user to restart
  └──────────────────────┘
```

1. On launch (5 seconds after startup) and when the user clicks **Check for
   Updates…**, the desktop app calls
   `GET https://modelrelay.io/updater/desktop/{target}/{arch}/{current_version}`.
   Tauri fills `{target}` in with `linux|darwin|windows`, `{arch}` with
   `x86_64|aarch64`, and `{current_version}` with the running app's semver.
2. The cloud service fetches the latest `desktop-v*` GitHub release (cached
   for 5 minutes), finds the `.app.tar.gz`/`.AppImage`/`-setup.exe` artifact
   matching the caller's target, reads its `.sig` file contents, and returns
   a Tauri-compatible JSON payload.
3. If `current_version` is already at or ahead of the latest release, the
   server returns `204 No Content` and the client does nothing.
4. If an update is available, the app verifies the minisign signature
   against the public key baked into `tauri.conf.json` before installing.
   A forged binary can't be installed even if the CDN is compromised.
5. After install, the user is prompted to restart. If they decline, the new
   binary loads next launch.

## Cutting a Release

Releases are gated on a `desktop-v*` git tag. The
[`desktop-release.yml`](../.github/workflows/desktop-release.yml) workflow
builds on Linux, macOS, and Windows runners, signs the updater artifacts
with the private key stored in GitHub Actions secrets, and uploads them
to the GitHub release.

```bash
# Bump version in crates/modelrelay-desktop/tauri.conf.json AND Cargo.toml
git tag desktop-v0.1.2
git push origin desktop-v0.1.2
```

The workflow will:

- Build `.dmg` / `.app.tar.gz` on macOS (both `x64` and `aarch64`)
- Build `.deb` / `.AppImage` on Linux (`x86_64`)
- Build `.msi` / `-setup.exe` on Windows (`x86_64`)
- Sign each updater artifact (`.app.tar.gz`, `.AppImage`, `-setup.exe`)
  and upload a matching `.sig` file
- Publish everything to the GitHub release

Within ~5 minutes (the server-side cache TTL), every running desktop app
will see the new version on its next check.

**Pre-release tags** (`-alpha`, `-beta`, `-rc` suffix) are marked as
GitHub pre-releases and are not returned by the updater endpoint — users
on stable builds won't be offered alpha updates.

## Required Secrets

The following GitHub Actions secrets must be set on the `ericflo/modelrelay`
repository for the release workflow to sign updater artifacts:

| Secret                                | Value                                                                |
|---------------------------------------|----------------------------------------------------------------------|
| `TAURI_SIGNING_PRIVATE_KEY`           | Contents of the minisign private key file (not a path).              |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`  | Password used when the keypair was generated (empty string if none). |

Without these, the workflow will still build the installers but won't
produce `.sig` files — and the cloud updater endpoint will then return
204 (no update) because it refuses to serve unsigned bundles.

## Generating / Rotating the Signing Key

The updater is only as trustworthy as the private key that signs bundles.
If the key is ever compromised (leaked, lost, shared outside CI), rotate
it immediately and cut a fresh release with the new key baked in.

```bash
# Install the Tauri CLI if you don't have it.
cargo install tauri-cli --version "^2"

# Generate a new keypair. Use a non-empty password for real releases.
cargo tauri signer generate --password "YOUR_PASSWORD" \
  --write-keys ./modelrelay-updater.key
```

This writes two files:

- `modelrelay-updater.key` — private key. **Keep out of git.** Copy its
  full contents into the `TAURI_SIGNING_PRIVATE_KEY` GitHub secret.
- `modelrelay-updater.key.pub` — public key. Paste its contents into
  `crates/modelrelay-desktop/tauri.conf.json` under `plugins.updater.pubkey`.

After rotating:

1. Commit the new pubkey change.
2. Publish a new `desktop-v*` release with the new private key configured
   in CI.
3. Users must install the new release manually (the old app doesn't trust
   the new key, so it can't auto-update across the rotation). Include a
   note in the release body explaining this.

## Server-Side Endpoint

The updater endpoint lives in
[`crates/modelrelay-cloud/src/routes/updater.rs`](../crates/modelrelay-cloud/src/routes/updater.rs).

- Route: `GET /updater/desktop/{target}/{arch}/{current_version}`
- 200 + JSON manifest when a newer release is available
- 204 No Content otherwise (also on parse errors, missing artifacts,
  unreachable GitHub API — clients will retry on the next check)
- Caches the latest GitHub release lookup for 5 minutes and the per-asset
  signature file contents for the process lifetime
- Unit tests cover semver comparison, platform suffix mapping, and the
  bundle/sig pair matcher

## Disabling Auto-Update

If a user wants to opt out (e.g. managed enterprise install), they can
quit the app and remove the `updater` entry from their preferences; the
updater won't fire without a user interaction after that. There's no
kill switch on the server side — if the endpoint returns 204 for long
enough (e.g. because releases stopped), the app simply keeps running the
current version.

## Troubleshooting

**"Update install failed: signature verification failed"**
The signing key in GitHub secrets doesn't match the `pubkey` in
`tauri.conf.json`. Regenerate both, or re-paste the pubkey.

**"Update check failed" in the settings UI**
The `/updater/desktop/...` endpoint is unreachable. Check
`modelrelay-cloud` logs; the endpoint is exempt from the session guard so
it should respond even if the DB is down.

**The app never offers an update after a release**
- Confirm the release tag starts with `desktop-v`.
- Confirm the `.sig` files are present on the release (the workflow
  uploads them automatically if signing secrets are configured).
- Wait up to 5 minutes for the server-side cache to expire, then restart
  the app.
- Compare `{target}/{arch}` in server logs against
  [`artifact_suffixes`](../crates/modelrelay-cloud/src/routes/updater.rs)
  — only the listed combinations are served.
