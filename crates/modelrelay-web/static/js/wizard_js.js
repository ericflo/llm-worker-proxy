
(function() {
  const STEPS = 8;
  let currentStep = 1;
  let detectedPlatform = 'linux';
  let selectedBackend = 'lmstudio';
  let workerPollInterval = null;
  let troubleshootTimer = null;
  let initialWorkerIds = new Set();
  let detectedModels = [];

  const $ = s => document.querySelector(s);
  const $$ = s => document.querySelectorAll(s);

  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) detectedPlatform = 'macos';
  else if (ua.includes('win')) detectedPlatform = 'windows';

  const cloudCfg = window.__mrCloudConfig || null;

  function getAdminToken() {
    if (cloudCfg) return '';
    return localStorage.getItem('mr_admin_token') || '';
  }
  function getServerUrl() {
    if (cloudCfg && cloudCfg.serverUrl) return cloudCfg.serverUrl;
    return localStorage.getItem('mr_server_url') || window.location.origin;
  }
  function getWorkersPollUrl() {
    if (cloudCfg && cloudCfg.workersPollUrl) return cloudCfg.workersPollUrl;
    return getServerUrl() + '/admin/workers';
  }
  function authHeaders() {
    const h = { 'Content-Type': 'application/json' };
    const t = getAdminToken();
    if (t) h['Authorization'] = 'Bearer ' + t;
    return h;
  }
  function escHtml(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }

  function goToStep(n) {
    if (n < 1 || n > STEPS) return;
    currentStep = n;
    $$('.wizard-step').forEach((el, i) => {
      el.classList.toggle('active', i + 1 === n);
    });
    $$('.step-indicator').forEach((el, i) => {
      el.classList.remove('active', 'done');
      if (i + 1 === n) el.classList.add('active');
      else if (i + 1 < n) el.classList.add('done');
    });
    if (n === 6) startWorkerPoll();
    else stopWorkerPoll();
  }

  function nextStep() { goToStep(currentStep + 1); }
  function prevStep() { goToStep(currentStep - 1); }
  window.__wizNext = nextStep;
  window.__wizPrev = prevStep;
  window.__wizGoTo = goToStep;

  // Platform tab switching (steps 1, 4)
  window.__setPlatform = function(p) {
    detectedPlatform = p;
    $$('.platform-tabs:not(.backend-tabs):not(.persist-tabs) .tab').forEach(t =>
      t.classList.toggle('active', t.dataset.platform === p));
    $$('.platform-content').forEach(el =>
      el.classList.toggle('active', el.dataset.platform === p));
    updateDownloadLinks();
    updateConfigSnippet();
    window.__setPersistPlatform(p);
  };

  // Backend tab switching (steps 2, 3)
  window.__setBackend = function(b) {
    selectedBackend = b;
    $$('.backend-tabs .tab').forEach(t =>
      t.classList.toggle('active', t.dataset.backend === b));
    $$('.backend-content').forEach(el =>
      el.classList.toggle('active', el.dataset.backend === b));
    updateConfigSnippet();
  };

  // Persist platform tabs (step 8)
  window.__setPersistPlatform = function(p) {
    $$('.persist-content').forEach(el =>
      el.style.display = el.dataset.platform === p ? 'block' : 'none');
    $$('.persist-tabs .tab').forEach(t =>
      t.classList.toggle('active', t.dataset.platform === p));
  };

  function updateDownloadLinks() {
    const base = 'https://github.com/ericflo/modelrelay/releases/latest/download';
    const binMap = {
      'macos': 'modelrelay-worker-darwin-arm64',
      'windows': 'modelrelay-worker-windows-amd64.exe',
      'linux': 'modelrelay-worker-linux-amd64',
    };
    const bin = binMap[detectedPlatform] || binMap['linux'];
    const el = $('#download-cmd');
    if (el) {
      if (detectedPlatform === 'windows') {
        el.textContent = 'curl -L -o modelrelay-worker.exe ' + base + '/' + bin;
      } else {
        el.textContent = 'curl -L -o modelrelay-worker ' + base + '/' + bin + ' && chmod +x modelrelay-worker';
      }
    }
  }

  function getBackendPort() {
    return selectedBackend === 'lmstudio' ? '1234' : '8000';
  }

  function updateConfigSnippet() {
    const serverUrl = $('#cfg-server-url') ? $('#cfg-server-url').value : getServerUrl();
    const secret = $('#cfg-worker-secret') ? $('#cfg-worker-secret').value : 'your-worker-secret';
    const workerName = $('#cfg-worker-name') ? ($('#cfg-worker-name').value || 'my-gpu-box') : 'my-gpu-box';
    const port = getBackendPort();
    const el = $('#config-toml');
    if (el) {
      el.textContent =
        'proxy_url = "' + serverUrl + '"\n' +
        'worker_secret = "' + secret + '"\n' +
        'worker_name = "' + workerName + '"\n' +
        'backend_url = "http://localhost:' + port + '"\n' +
        'models = ["*"]';
    }
    // Also update env var snippet
    const envEl = $('#config-env');
    if (envEl) {
      envEl.textContent =
        'export PROXY_URL="' + serverUrl + '"\n' +
        'export WORKER_SECRET="' + secret + '"\n' +
        'export WORKER_NAME="' + workerName + '"\n' +
        'export BACKEND_URL="http://localhost:' + port + '"\n' +
        'export MODELS="*"';
    }
    // Update curl test command
    const curlEl = $('#curl-test');
    if (curlEl) {
      const apiKeyInput = $('#test-api-key');
      const apiKey = (cloudCfg && cloudCfg.apiKey) ? cloudCfg.apiKey : (apiKeyInput ? apiKeyInput.value.trim() : '') || (localStorage.getItem('mr_test_api_key') || '');
      const testModel = ($('#test-model') && $('#test-model').value.trim()) || 'your-model';
      let curlCmd = 'curl -X POST ' + serverUrl + '/v1/chat/completions \\\n' +
        '  -H "Content-Type: application/json" \\\n';
      if (apiKey) curlCmd += '  -H "Authorization: Bearer ' + apiKey + '" \\\n';
      curlCmd += '  -d \'{"model":"' + testModel + '","messages":[{"role":"user","content":"Hello!"}],"max_tokens":100}\'';
      curlEl.textContent = curlCmd;
    }
  }
  window.__updateConfig = updateConfigSnippet;

  // Step 6: poll for new worker
  async function snapshotWorkers() {
    try {
      const pollUrl = getWorkersPollUrl();
      const opts = cloudCfg ? { credentials: 'same-origin' } : { headers: authHeaders() };
      const r = await fetch(pollUrl, opts);
      if (!r.ok) return;
      const d = await r.json();
      (d.workers || []).forEach(w => initialWorkerIds.add(w.worker_id));
    } catch(e) {}
  }

  function startWorkerPoll() {
    stopWorkerPoll();
    snapshotWorkers();
    const pulse = $('#worker-pulse');
    const statusText = $('#worker-status-text');
    const troubleshoot = $('#troubleshoot-hints');
    const skipBtn = $('#skip-detect');
    if (pulse) pulse.className = 'pulse searching';
    if (statusText) statusText.textContent = 'Waiting for worker to connect...';
    if (troubleshoot) troubleshoot.style.display = 'none';
    if (skipBtn) skipBtn.style.display = 'none';

    // Show troubleshooting after 15s, skip button after 30s
    let elapsed = 0;
    troubleshootTimer = setInterval(() => {
      elapsed += 1;
      if (elapsed >= 15 && troubleshoot) troubleshoot.style.display = 'block';
      if (elapsed >= 30 && skipBtn) skipBtn.style.display = 'inline-block';
    }, 1000);

    workerPollInterval = setInterval(async () => {
      try {
        const pollUrl = getWorkersPollUrl();
        const opts = cloudCfg ? { credentials: 'same-origin' } : { headers: authHeaders() };
        const r = await fetch(pollUrl, opts);
        if (!r.ok) return;
        const d = await r.json();
        const workers = d.workers || [];
        const newWorker = workers.find(w => !initialWorkerIds.has(w.worker_id));
        if (newWorker) {
          stopWorkerPoll();
          if (pulse) pulse.className = 'pulse connected';
          if (statusText) {
            const name = newWorker.worker_name || newWorker.worker_id;
            const models = (newWorker.models || []).join(', ');
            detectedModels = newWorker.models || [];
            const modelsDisplay = models === '*' ? 'all models' : models;
            statusText.innerHTML = '<span class="check-mark">&#10003;</span> Worker <strong>' + escHtml(name) + '</strong> connected!' +
              (modelsDisplay ? ' <span style="color:#8b949e;">(' + escHtml(modelsDisplay) + ')</span>' : '');
          }
          if (troubleshoot) troubleshoot.style.display = 'none';
          if (skipBtn) skipBtn.style.display = 'none';
          const nextBtn = $('#step6-next');
          if (nextBtn) { nextBtn.disabled = false; nextBtn.style.opacity = '1'; }
          // Pre-fill test model from detected models
          const testModel = $('#test-model');
          if (testModel && detectedModels.length > 0 && !testModel.value) {
            testModel.value = detectedModels[0];
          }
        }
      } catch(e) {}
    }, 3000);
  }

  function stopWorkerPoll() {
    if (workerPollInterval) { clearInterval(workerPollInterval); workerPollInterval = null; }
    if (troubleshootTimer) { clearInterval(troubleshootTimer); troubleshootTimer = null; }
  }

  // Step 7: test inference
  window.__testInference = async function() {
    const resultEl = $('#test-result');
    const btnEl = $('#test-btn');
    if (!resultEl || !btnEl) return;
    btnEl.disabled = true;
    btnEl.textContent = 'Sending...';
    resultEl.textContent = 'Sending request...';
    resultEl.style.display = 'block';

    const serverUrl = getServerUrl();
    const model = $('#test-model') ? $('#test-model').value.trim() : 'default';
    const body = {
      model: model || 'default',
      messages: [{ role: 'user', content: 'Hello! Reply in one short sentence.' }],
      max_tokens: 100,
    };

    try {
      const apiKeyInput = $('#test-api-key');
      let apiKey = (cloudCfg && cloudCfg.apiKey) ? cloudCfg.apiKey : '';
      if (!apiKey && apiKeyInput) apiKey = apiKeyInput.value.trim();
      if (!apiKey) apiKey = localStorage.getItem('mr_test_api_key') || '';
      if (apiKey && apiKeyInput) localStorage.setItem('mr_test_api_key', apiKey);
      const headers = { 'Content-Type': 'application/json' };
      if (apiKey) headers['Authorization'] = 'Bearer ' + apiKey;
      const r = await fetch(serverUrl + '/v1/chat/completions', {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      });
      const text = await r.text();
      if (r.ok) {
        try {
          const d = JSON.parse(text);
          const reply = d.choices && d.choices[0] && d.choices[0].message
            ? d.choices[0].message.content : text;
          resultEl.innerHTML = '<span class="check-mark">&#10003;</span> <strong>Success!</strong>\n\n'
            + 'Model: ' + escHtml(d.model || model) + '\n'
            + 'Response: ' + escHtml(reply);
        } catch(e) {
          resultEl.textContent = 'Response (raw):\n' + text;
        }
      } else {
        resultEl.textContent = 'Error ' + r.status + ':\n' + text;
      }
    } catch(e) {
      resultEl.textContent = 'Connection failed: ' + e.message;
    }
    btnEl.disabled = false;
    btnEl.textContent = 'Send Test Request';
  };

  window.__copyCode = function(id) {
    const el = document.getElementById(id);
    if (el) navigator.clipboard.writeText(el.textContent);
  };

  // Init
  goToStep(1);
  window.__setPlatform(detectedPlatform);
  window.__setBackend('lmstudio');
  window.__setPersistPlatform(detectedPlatform);

  // Pre-fill config inputs
  if (cloudCfg) {
    const urlInput = $('#cfg-server-url');
    if (urlInput && cloudCfg.serverUrl) urlInput.value = cloudCfg.serverUrl;
    const secretInput = $('#cfg-worker-secret');
    if (secretInput && cloudCfg.workerSecret) secretInput.value = cloudCfg.workerSecret;
    const apiKeyInput = $('#test-api-key');
    if (apiKeyInput && cloudCfg.apiKey) apiKeyInput.value = cloudCfg.apiKey;
    updateConfigSnippet();
  } else {
    const urlInput = $('#cfg-server-url');
    if (urlInput && !urlInput.value) urlInput.value = window.location.origin;
    const savedApiKey = localStorage.getItem('mr_test_api_key');
    const apiKeyInput = $('#test-api-key');
    if (apiKeyInput && savedApiKey && !apiKeyInput.value) apiKeyInput.value = savedApiKey;
  }

  document.addEventListener('input', (e) => {
    if (e.target.id === 'cfg-server-url' || e.target.id === 'cfg-worker-secret' || e.target.id === 'cfg-worker-name' || e.target.id === 'test-api-key' || e.target.id === 'test-model') {
      updateConfigSnippet();
    }
  });
})();
    "#;

    let step_labels = [
        "Platform",
        "Backend",
        "Model",
        "Download",
        "Configure",
        "Connect",
        "Test",
        "Persist",
    ];

    let mut progress_html = String::from("<div class=\"wizard-progress\">");
    for (i, label) in step_labels.iter().enumerate() {
        let cls = if i == 0 { " active" } else { "" };
        let step_num = i + 1;
        let _ = write!(
            progress_html,
            "<div class=\"step-indicator{cls}\" onclick=\"window.__wizGoTo({step_num})\">{label}</div>"
        );
    }
    progress_html.push_str("</div>");

    let steps_html = r#