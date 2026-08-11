
(function() {
  const POLL_MS = 4000;
  let adminToken = localStorage.getItem('mr_admin_token') || '';
  let serverUrl = localStorage.getItem('mr_server_url') || '';

  const $ = (s) => document.querySelector(s);
  const $$ = (s) => document.querySelectorAll(s);

  function baseUrl() {
    return serverUrl || window.location.origin;
  }

  function authHeaders() {
    const h = { 'Content-Type': 'application/json' };
    if (adminToken) h['Authorization'] = 'Bearer ' + adminToken;
    return h;
  }

  function fmtDuration(secs) {
    if (secs < 60) return Math.floor(secs) + 's';
    if (secs < 3600) return Math.floor(secs/60) + 'm ' + Math.floor(secs%60) + 's';
    const h = Math.floor(secs/3600);
    const m = Math.floor((secs%3600)/60);
    return h + 'h ' + m + 'm';
  }

  function fmtTimestamp(ts) {
    if (!ts) return '—';
    return new Date(ts * 1000).toLocaleString();
  }

  function escHtml(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }

  // --- Config bar ---
  function initConfig() {
    const tokenInput = $('#cfg-token');
    const urlInput = $('#cfg-url');
    const status = $('#cfg-status');
    tokenInput.value = adminToken;
    urlInput.value = serverUrl;

    tokenInput.addEventListener('change', () => {
      adminToken = tokenInput.value.trim();
      localStorage.setItem('mr_admin_token', adminToken);
      pollAll();
    });
    urlInput.addEventListener('change', () => {
      serverUrl = urlInput.value.trim().replace(/\/+$/, '');
      localStorage.setItem('mr_server_url', serverUrl);
      pollAll();
    });
  }

  // --- Health ---
  async function pollHealth() {
    try {
      const r = await fetch(baseUrl() + '/health');
      if (!r.ok) throw new Error(r.status);
      const d = await r.json();
      $('#h-status').textContent = d.status || '—';
      $('#h-status').className = 'value ' + (d.status === 'ok' ? 'ok' : 'warn');
      $('#h-version').textContent = d.version || '—';
      $('#h-version').className = 'value ok';
      $('#h-workers').textContent = d.workers_connected ?? '—';
      $('#h-workers').className = 'value ' + ((d.workers_connected||0) > 0 ? 'ok' : 'warn');
      $('#h-queue').textContent = d.queue_depth ?? '—';
      $('#h-queue').className = 'value ' + ((d.queue_depth||0) > 0 ? 'warn' : 'ok');
      $('#h-uptime').textContent = fmtDuration(d.uptime_secs || 0);
      $('#h-uptime').className = 'value ok';
      $('#cfg-status').textContent = 'Connected';
      $('#cfg-status').className = 'status ok';
    } catch(e) {
      $('#h-status').textContent = 'error';
      $('#h-status').className = 'value err';
      $('#cfg-status').textContent = 'Connection failed';
      $('#cfg-status').className = 'status fail';
    }
  }

  // --- Workers ---
  async function pollWorkers() {
    const el = $('#workers-body');
    if (!adminToken) {
      el.innerHTML = '<div class="empty-state">Enter admin token above to view workers.</div>';
      return;
    }
    try {
      const r = await fetch(baseUrl() + '/admin/workers', { headers: authHeaders() });
      if (r.status === 403) {
        el.innerHTML = '<div class="empty-state" style="color:#f87171;">Invalid admin token.</div>';
        return;
      }
      if (!r.ok) throw new Error(r.status);
      const d = await r.json();
      const workers = d.workers || [];
      if (workers.length === 0) {
        el.innerHTML = '<div class="empty-state">No workers connected.<br><a href="/setup" class="btn-sm" style="margin-top:8px;display:inline-block;">Set up your first worker &rarr;</a></div>';
        return;
      }
      let html = '<table class="data"><thead><tr><th>Worker</th><th>Models</th><th>Load</th><th>Status</th></tr></thead><tbody>';
      for (const w of workers) {
        const models = (w.models||[]).map(m => '<span class="model-tag">' + (m === '*' ? 'All models' : escHtml(m)) + '</span>').join('');
        const load = w.in_flight_count + ' / ' + w.max_concurrent;
        const status = w.is_draining
          ? '<span class="badge badge-warn">Draining</span>'
          : '<span class="badge badge-active">Active</span>';
        html += '<tr><td>' + escHtml(w.worker_name || w.worker_id) + '</td><td>' + models + '</td><td>' + load + '</td><td>' + status + '</td></tr>';
      }
      html += '</tbody></table>';
      el.innerHTML = html;
    } catch(e) {
      el.innerHTML = '<div class="empty-state" style="color:#f87171;">Failed to load workers.</div>';
    }
  }

  // --- Stats ---
  async function pollStats() {
    const el = $('#stats-body');
    if (!adminToken) {
      el.innerHTML = '<div class="empty-state">Enter admin token above to view stats.</div>';
      return;
    }
    try {
      const r = await fetch(baseUrl() + '/admin/stats', { headers: authHeaders() });
      if (r.status === 403) {
        el.innerHTML = '<div class="empty-state" style="color:#f87171;">Invalid admin token.</div>';
        return;
      }
      if (!r.ok) throw new Error(r.status);
      const d = await r.json();
      const qd = d.queue_depth || {};
      const models = Object.keys(qd);
      let html = '<div style="margin-bottom:12px;color:#8b949e;font-size:0.9rem;">Active workers: <strong style="color:#e6edf3;">' + (d.active_workers||0) + '</strong></div>';
      if (models.length === 0) {
        html += '<div class="empty-state">No models queued.</div>';
      } else {
        const maxQ = Math.max(1, ...models.map(m => qd[m]));
        for (const m of models) {
          const pct = Math.round((qd[m] / maxQ) * 100);
          html += '<div class="stat-row"><span class="stat-label">' + escHtml(m) + '</span>'
            + '<div class="stat-bar"><div class="stat-fill" style="width:' + pct + '%"></div></div>'
            + '<span class="stat-num">' + qd[m] + '</span></div>';
        }
      }
      el.innerHTML = html;
    } catch(e) {
      el.innerHTML = '<div class="empty-state" style="color:#f87171;">Failed to load stats.</div>';
    }
  }

  // --- API Keys ---
  async function pollKeys() {
    const el = $('#keys-body');
    if (!adminToken) {
      el.innerHTML = '<div class="empty-state">Enter admin token above to manage API keys.</div>';
      return;
    }
    try {
      const r = await fetch(baseUrl() + '/admin/keys', { headers: authHeaders() });
      if (r.status === 403) {
        el.innerHTML = '<div class="empty-state" style="color:#f87171;">Invalid admin token.</div>';
        return;
      }
      if (!r.ok) throw new Error(r.status);
      const d = await r.json();
      const keys = d.keys || [];
      if (keys.length === 0) {
        el.innerHTML = '<div class="empty-state">No API keys created yet.</div>';
        return;
      }
      let html = '<table class="data"><thead><tr><th>Name</th><th>Prefix</th><th>Created</th><th>Last Used</th><th>Status</th><th></th></tr></thead><tbody>';
      for (const k of keys) {
        const status = k.revoked
          ? '<span class="badge badge-cancel">Revoked</span>'
          : '<span class="badge badge-active">Active</span>';
        const revokeBtn = k.revoked ? '' : '<button class="btn-sm danger" onclick="window.__revokeKey(\'' + escHtml(k.id) + '\',\'' + escHtml(k.name) + '\')">Revoke</button>';
        html += '<tr><td>' + escHtml(k.name) + '</td><td><code>' + escHtml(k.prefix) + '...</code></td>'
          + '<td>' + fmtTimestamp(k.created_at) + '</td>'
          + '<td>' + fmtTimestamp(k.last_used_at) + '</td>'
          + '<td>' + status + '</td><td>' + revokeBtn + '</td></tr>';
      }
      html += '</tbody></table>';
      el.innerHTML = html;
    } catch(e) {
      el.innerHTML = '<div class="empty-state" style="color:#f87171;">Failed to load keys.</div>';
    }
  }

  window.__createKey = async function() {
    const nameInput = $('#new-key-name');
    const name = nameInput.value.trim();
    if (!name) { nameInput.focus(); return; }
    try {
      const r = await fetch(baseUrl() + '/admin/keys', {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify({ name }),
      });
      if (!r.ok) throw new Error(r.status);
      const d = await r.json();
      nameInput.value = '';
      $('#new-key-secret').innerHTML = '<div class="secret-box">'
        + '<button class="copy-btn" onclick="navigator.clipboard.writeText(\'' + escHtml(d.secret) + '\')">Copy</button>'
        + escHtml(d.secret)
        + '<span class="warn">&#9888; This secret will not be shown again. Copy it now.</span></div>';
      pollKeys();
    } catch(e) {
      alert('Failed to create key: ' + e.message);
    }
  };

  window.__revokeKey = async function(id, name) {
    if (!confirm('Revoke API key "' + name + '"? This cannot be undone.')) return;
    try {
      const r = await fetch(baseUrl() + '/admin/keys/' + encodeURIComponent(id), {
        method: 'DELETE',
        headers: authHeaders(),
      });
      if (!r.ok && r.status !== 204) throw new Error(r.status);
      pollKeys();
    } catch(e) {
      alert('Failed to revoke key: ' + e.message);
    }
  };

  async function pollAll() {
    await Promise.all([pollHealth(), pollWorkers(), pollStats(), pollKeys()]);
  }

  initConfig();
  pollAll();
  setInterval(pollAll, POLL_MS);
})();
    "#;

    let body_content = r