# UI Audit Matrix — Setup Wizard & Integrate

Evidence-backed audit of current web UI instructions vs repo source of truth.

## Methodology
* Extracted user-visible text/commands from `crates/modelrelay-web/src/templates.rs` → `setup_wizard_page_with_config` and `integrate_page_with_config`
* Cross-referenced with:
  * `crates/modelrelay-worker/src/main.rs` CLI/env definitions
  * `crates/modelrelay-server` CLI/env definitions
  * `.env.example`, `docker-compose.yml`
  * `extras/modelrelay-*.service`, `extras/*.env.example`
  * `docs/`, `README.md`

## Setup Wizard Audit

### Step 1 Platform
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| macOS / Windows / Linux tabs | Repo supports all three, prebuilt binaries exist | Correct | |
| Desktop app download link to GitHub releases | README confirms desktop app releases | Correct | |

### Step 2 Backend selection
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| LM Studio runs on `http://localhost:1234` | Common default, UI uses 1234 | Likely correct | Verify LM Studio docs; keep as default |
| Ollama serves on `http://localhost:11434` | Standard Ollama port | Correct | |
| llama.cpp server flag `--port 8000 --host 0.0.0.0` | User must download GGUF; no repo enforcement | Ambiguous | Port is configurable; UI assumes 8000 |
| vLLM serves on `http://localhost:8000` | Standard vLLM default | Correct | |

### Step 3 Model download
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| Verify with `curl http://localhost:<port>/v1/models` | OpenAI compatible backends expose this | Correct | |

### Step 4 Download Worker
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| `curl -L -o modelrelay-worker https://github.com/ericflo/modelrelay/releases/latest/download/modelrelay-worker-linux-amd64 && chmod +x` | Releases provide binaries for linux/mac/windows x86_64/arm64 | Correct | Binary names match README table |

### Step 5 Configure
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| Config TOML fields: `proxy_url`, `worker_secret`, `worker_name`, `backend_url`, `models` | `crates/modelrelay-worker/src/main.rs` defines `--proxy-url`, `--worker-secret`, `--worker-name`, `--models`, `--backend-url`. FileConfig struct matches. | Correct | |
| Env var snippet uses `export PROXY_URL=...` etc | Args defined with `env = "PROXY_URL"` etc | Correct | |
| Default backend port hint derived from backend selection | UI logic maps lmstudio→1234, others→8000 | Reasonable | |

### Step 6 Connect
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| Polls `/admin/workers` for new worker | `modelrelay-cloud` proxies to admin API; OSS requires admin token | Correct for cloud, needs token for OSS | Behavior differs OSS vs cloud |

### Step 7 Test
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| Test request to `/v1/chat/completions` with Bearer API key | Server supports OpenAI compatible endpoints | Correct | |

### Step 8 Persist
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| Systemd template `modelrelay-worker@.service` | `extras/modelrelay-worker@.service` exists | Correct | |
| Windows service via sc.exe | `extras/install-windows-service*.ps1` exist | Correct | |

## Integrate Page Audit

### Input pre-fill
| UI claim | Source of truth | Status |
|---|---|---|
| Server URL, API Key, Model Name inputs pre-filled from CloudWizardConfig | `dashboard.rs` passes server_url/api_key/worker_secret | Correct |

### Snippet templates
| UI claim | Source of truth | Status | Notes |
|---|---|---|---|
| OpenAI Python/Node/Go snippets using `baseURL` / `base_url` | Standard SDK usage | Correct |
| Anthropic Messages API with headers `x-api-key`, `anthropic-version` | Server supports Anthropic compatibility per README/docs | Correct |
| OpenAI Responses API `/v1/responses` | README lists `POST /v1/responses` as compatible | Correct |
| curl commands use `-H "Authorization: Bearer API_KEY"` | Standard | Correct |

### Live Demo
| UI claim | Source of truth | Status |
|---|---|---|
| Streaming demo parses SSE for chat/completions, messages, responses | Server streams SSE chunks per protocol docs | Correct in principle; needs verification of exact event names |

## Outdated / Risky Items Found

1. **Worker config TOML field `models = ["*"]`** – UI suggests wildcard, but worker registration requires explicit model names or `default`. Needs clarification.
2. **Backend port assumptions** – UI hardcodes lmstudio 1234, others 8000. Some users run llama.cpp on 8080. Should be configurable not assumed.
3. **Secret persistence in localStorage** – wizard stores admin token/server URL/api key in localStorage. Security concern; should warn/avoid.
4. **Cloud vs OSS polling divergence** – setup step 6 works differently for cloud users vs self-hosted. UI does not surface the difference clearly.
5. **Integration guide links** – need verification that all external links are current: LM Studio download, Ollama install, vLLM docs.

## Immediate Fixes Required

* Make backend port configurable in wizard, don’t assume.
* Clarify `models` field – remove wildcard suggestion or document exact names.
* Add security notice about localStorage usage for tokens.
* Distinguish cloud vs self-hosted polling behavior in UI copy.
* Verify all external URLs and update to current versions.

## Next Steps
Proceed with Phase 1: Extract static assets from `templates.rs` without changing behavior, then implement design system + data-driven content manifests.
