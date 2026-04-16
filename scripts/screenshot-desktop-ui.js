#!/usr/bin/env node
// Renders crates/modelrelay-desktop/ui/index.html in headless Chromium
// and captures PNGs of the dashboard, settings, and onboarding panes.
// Tauri APIs are stubbed with mock data for static rendering.
//
// Usage (from repo root):
//   npm install puppeteer
//   node scripts/screenshot-desktop-ui.js
//
// Outputs PNGs to docs/screenshots/desktop/. Used to refresh the README
// screenshots whenever the desktop UI changes materially.

const http = require("http");
const fs = require("fs");
const path = require("path");
const puppeteer = require("puppeteer");

const UI_DIR = path.resolve(__dirname, "../crates/modelrelay-desktop/ui");
const OUT_DIR = path.resolve(__dirname, "../docs/screenshots/desktop");

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

function serveUi(port) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const url = (req.url || "/").split("?")[0];
      const rel = url === "/" ? "/index.html" : url;
      const filePath = path.join(UI_DIR, rel);
      if (!filePath.startsWith(UI_DIR)) {
        res.writeHead(403);
        res.end("forbidden");
        return;
      }
      fs.readFile(filePath, (err, data) => {
        if (err) {
          res.writeHead(404);
          res.end("not found");
          return;
        }
        const ext = path.extname(filePath).toLowerCase();
        res.writeHead(200, { "Content-Type": MIME[ext] || "text/plain" });
        res.end(data);
      });
    });
    server.listen(port, "127.0.0.1", () => resolve(server));
  });
}

// Mock Tauri API with realistic sample data.
function buildTauriStub({ connected, models, hasSettings, settings, error }) {
  return `
    (function () {
      const mockStatus = {
        connected: ${connected},
        relay_url: ${JSON.stringify(settings.relay_url)},
        active_requests: ${connected ? 2 : 0},
        models: ${JSON.stringify(models)},
        error: ${error ? JSON.stringify(error) : "null"}
      };
      const mockSettings = ${JSON.stringify(settings)};
      const hasSaved = ${hasSettings};

      const commands = {
        get_status: async () => mockStatus,
        get_settings: async () => mockSettings,
        save_settings: async () => null,
        start_worker: async () => null,
        stop_worker: async () => null,
        get_has_saved_settings: async () => hasSaved,
      };

      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            const fn = commands[cmd];
            if (!fn) throw new Error("unknown command: " + cmd);
            return fn(args);
          }
        },
        webviewWindow: {
          // Return an object with listen() that never fires — enough for setup code.
          // The real app uses this to react to tray menu clicks.
        }
      };
      // Fake webviewWindow needs a getCurrentWebviewWindow function that returns
      // an object with a .listen() method. Provide a quiet no-op implementation.
      window.__TAURI__.webviewWindow.getCurrentWebviewWindow = () => ({
        listen: () => Promise.resolve(() => {})
      });
    })();
  `;
}

const SAMPLE_SETTINGS = {
  backend_url: "http://localhost:11434",
  relay_url: "https://api.modelrelay.io",
  worker_secret: "mr_live_sk_8f3a2c1b9e7d4a5f6c8b0d2e1f3a5b7c",
  provider: "home-rig",
  worker_name: "workstation-3090",
  models: ["llama-3.3-70b", "qwen2.5-coder-32b", "mistral-small-24b"],
  max_concurrent: 4,
  poll_interval_secs: 30,
  auto_start: true,
};

const SAMPLE_MODELS = [
  "llama-3.3-70b",
  "qwen2.5-coder-32b",
  "mistral-small-24b",
  "deepseek-r1-distill-llama-70b",
  "phi-4-14b",
  "gemma-2-27b",
];

async function screenshot(browser, { name, stubOpts, afterLoad, url }) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 780, deviceScaleFactor: 2 });

  const stub = buildTauriStub(stubOpts);
  await page.evaluateOnNewDocument(stub);

  await page.goto(url, { waitUntil: "networkidle0", timeout: 20000 });
  // Let any setInterval tick once and styles settle.
  await new Promise((r) => setTimeout(r, 500));

  if (afterLoad) {
    await afterLoad(page);
    await new Promise((r) => setTimeout(r, 400));
  }

  const outFile = path.join(OUT_DIR, name + ".png");
  await page.screenshot({ path: outFile, fullPage: false });
  const size = fs.statSync(outFile).size;
  console.log(`Wrote ${outFile} (${(size / 1024).toFixed(1)} KB)`);
  await page.close();
}

(async () => {
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const port = 17931;
  const server = await serveUi(port);
  const baseUrl = `http://127.0.0.1:${port}/index.html`;

  const browser = await puppeteer.launch({
    executablePath: "/usr/bin/chromium-browser",
    headless: true,
    args: ["--no-sandbox", "--disable-dev-shm-usage"],
  });

  try {
    // Dashboard — connected worker with a healthy model list.
    await screenshot(browser, {
      name: "dashboard",
      url: baseUrl,
      stubOpts: {
        connected: true,
        models: SAMPLE_MODELS,
        hasSettings: true,
        settings: SAMPLE_SETTINGS,
      },
      afterLoad: async (page) => {
        // Ensure the dashboard tab is active; it's the default.
        await page.evaluate(() => {
          const t = document.querySelector('[data-tab="dashboard"]');
          if (t) t.click();
        });
      },
    });

    // Settings — same mocked settings, switch to Settings tab.
    await screenshot(browser, {
      name: "settings",
      url: baseUrl,
      stubOpts: {
        connected: true,
        models: SAMPLE_MODELS,
        hasSettings: true,
        settings: SAMPLE_SETTINGS,
      },
      afterLoad: async (page) => {
        await page.evaluate(() => {
          const t = document.querySelector('[data-tab="settings"]');
          if (t) t.click();
        });
      },
    });

    // Onboarding — shown when no settings have been saved.
    // Land on step 2 (test connection) with a successful result for maximum visual interest.
    await screenshot(browser, {
      name: "onboarding",
      url: baseUrl,
      stubOpts: {
        connected: false,
        models: [],
        hasSettings: false,
        settings: SAMPLE_SETTINGS,
      },
      afterLoad: async (page) => {
        // Fill earlier steps so they feel realistic if anyone clicks Back,
        // and advance the wizard via its own Next button so the closure-scoped
        // wizardStep variable is updated correctly.
        await page.evaluate(() => {
          document.getElementById("wizBackendUrl").value = "http://localhost:11434";
          document.getElementById("wizRelayUrl").value = "https://api.modelrelay.io";
          document.getElementById("wizWorkerSecret").value = "mr_live_sk_8f3a2c1b9e7d4a5f6c8b0d2e1f3a5b7c";
          // Click Next twice: step 0 -> 1 -> 2.
          window.wizardNext();
          window.wizardNext();

          // Fake a successful test result on step 2.
          const resultEl = document.getElementById("wizTestResult");
          resultEl.className = "test-result success";
          resultEl.textContent =
            "\u2713 Connected successfully! Models found: llama-3.3-70b, qwen2.5-coder-32b, mistral-small-24b";
          document.getElementById("wizFinishBtn").disabled = false;
        });
      },
    });
  } finally {
    await browser.close();
    server.close();
  }
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
