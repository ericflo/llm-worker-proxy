
(function() {
  const $ = s => document.querySelector(s);
  const $$ = s => document.querySelectorAll(s);

  const cloudCfg = window.__mrCloudConfig || null;

  // ── Inputs ──
  const urlInput = $('#int-server-url');
  const keyInput = $('#int-api-key');
  const modelInput = $('#int-model-name');

  // Pre-fill from cloud config or localStorage
  urlInput.value = (cloudCfg && cloudCfg.serverUrl) ? cloudCfg.serverUrl
    : localStorage.getItem('mr_server_url') || window.location.origin;
  keyInput.value = (cloudCfg && cloudCfg.apiKey) ? cloudCfg.apiKey
    : localStorage.getItem('mr_test_api_key') || '';
  modelInput.value = localStorage.getItem('mr_model_name') || '';

  // Show cloud banner when logged in with pre-filled credentials
  if (cloudCfg && cloudCfg.apiKey) {
    const banner = $('#int-cloud-banner');
    if (banner) banner.style.display = 'block';
  }

  function sv() { return urlInput.value.trim().replace(/\/+$/, '') || 'https://your-server.example.com'; }
  function ak() { return keyInput.value.trim() || 'your-api-key'; }
  function mn() { return modelInput.value.trim() || 'your-model-name'; }

  // ── Tab switching ──
  function initTabs(section) {
    const tabs = section.querySelectorAll('.int-tabs .tab');
    const panels = section.querySelectorAll('.int-content');
    tabs.forEach(tab => {
      tab.addEventListener('click', () => {
        tabs.forEach(t => t.classList.remove('active'));
        panels.forEach(p => p.classList.remove('active'));
        tab.classList.add('active');
        const target = section.querySelector('.int-content[data-tab="' + tab.dataset.tab + '"]');
        if (target) target.classList.add('active');
        updateSnippets();
      });
    });
  }
  $$('.tab-section').forEach(initTabs);

  // ── Copy button ──
  document.addEventListener('click', e => {
    const btn = e.target.closest('.copy-btn');
    if (!btn) return;
    const block = btn.closest('.code-block') || btn.closest('.ref-value') || btn.closest('code');
    if (!block) return;
    const clone = block.cloneNode(true);
    clone.querySelectorAll('.copy-btn').forEach(b => b.remove());
    const text = clone.textContent.trim();
    navigator.clipboard.writeText(text).then(() => {
      const prev = btn.textContent;
      btn.textContent = '\u2713 Copied!';
      btn.classList.add('copied');
      setTimeout(() => { btn.textContent = prev; btn.classList.remove('copied'); }, 1500);
    });
  });

  // ── Snippet updater ──
  function updateSnippets() {
    const s = sv(), a = ak(), m = mn();
    // Agent snippets
    $$('[data-snippet]').forEach(el => {
      const tpl = el.getAttribute('data-snippet');
      el.querySelector('.code-text').innerHTML = tpl
        .replace(/SERVER_URL/g, escHtml(s))
        .replace(/API_KEY/g, escHtml(a))
        .replace(/MODEL_NAME/g, escHtml(m));
    });
    // Ref values
    $$('[data-ref]').forEach(el => {
      const tpl = el.getAttribute('data-ref');
      const span = el.querySelector('.ref-val');
      if (span) span.textContent = tpl.replace(/SERVER_URL/g, s).replace(/API_KEY/g, a);
    });
  }

  function escHtml(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
  }

  urlInput.addEventListener('input', () => {
    localStorage.setItem('mr_server_url', urlInput.value.trim());
    updateSnippets();
  });
  keyInput.addEventListener('input', () => {
    localStorage.setItem('mr_test_api_key', keyInput.value.trim());
    updateSnippets();
  });
  modelInput.addEventListener('input', () => {
    localStorage.setItem('mr_model_name', modelInput.value.trim());
    updateSnippets();
  });

  updateSnippets();

  // ── Live Demo ──
  const demoPrompt = $('#demo-prompt');
  const demoSend = $('#demo-send');
  const demoStop = $('#demo-stop');
  const demoOutput = $('#demo-output');
  const demoStatus = $('#demo-status');
  const demoStreamToggle = $('#demo-stream-toggle');
  const demoApiFormat = $('#demo-api-format');
  let demoAbort = null;

  demoPrompt.addEventListener('keydown', e => {
    if (e.key === 'Enter' && !demoSend.disabled) runDemo();
  });
  demoSend.addEventListener('click', runDemo);
  demoStop.addEventListener('click', () => {
    if (demoAbort) demoAbort.abort();
  });

  function buildDemoRequest(format, url, key, model, prompt, streaming) {
    if (format === 'messages') {
      return {
        endpoint: url + '/v1/messages',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': key,
          'anthropic-version': '2023-06-01',
        },
        body: { model: model, max_tokens: 1024, messages: [{ role: 'user', content: prompt }], stream: streaming },
      };
    } else if (format === 'responses') {
      return {
        endpoint: url + '/v1/responses',
        headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + key },
        body: { model: model, input: prompt, stream: streaming },
      };
    }
    // default: chat completions
    return {
      endpoint: url + '/v1/chat/completions',
      headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + key },
      body: { model: model, messages: [{ role: 'user', content: prompt }], stream: streaming },
    };
  }

  function extractNonStreamContent(format, data) {
    if (format === 'messages') return data.content?.[0]?.text || '(empty response)';
    if (format === 'responses') return data.output?.[0]?.content?.[0]?.text || data.output_text || '(empty response)';
    return data.choices?.[0]?.message?.content || '(empty response)';
  }

  function extractStreamDelta(format, line) {
    if (format === 'messages') {
      // Anthropic SSE: look for content_block_delta events
      if (!line.startsWith('data: ')) return null;
      const payload = line.slice(6).trim();
      if (payload === '[DONE]') return null;
      try {
        const chunk = JSON.parse(payload);
        if (chunk.type === 'content_block_delta' && chunk.delta?.text) return chunk.delta.text;
      } catch(_) {}
      return null;
    }
    if (format === 'responses') {
      // Responses API SSE: look for response.output_text.delta events
      if (!line.startsWith('data: ')) return null;
      const payload = line.slice(6).trim();
      if (payload === '[DONE]') return null;
      try {
        const chunk = JSON.parse(payload);
        if (chunk.type === 'response.output_text.delta' && chunk.delta) return chunk.delta;
      } catch(_) {}
      return null;
    }
    // Chat completions
    if (!line.startsWith('data: ')) return null;
    const payload = line.slice(6).trim();
    if (payload === '[DONE]') return null;
    try {
      const chunk = JSON.parse(payload);
      return chunk.choices?.[0]?.delta?.content || null;
    } catch(_) {}
    return null;
  }

  async function runDemo() {
    const url = sv();
    const key = ak();
    const model = mn();
    const format = demoApiFormat.value;
    if (!url || url === 'https://your-server.example.com') {
      demoOutput.innerHTML = '<span class="demo-placeholder">Enter your server URL above to try the live demo.</span>';
      return;
    }
    if (!key || key === 'your-api-key') {
      demoOutput.innerHTML = '<span class="demo-placeholder">Enter your API key above to try the live demo.</span>';
      return;
    }
    if (!model || model === 'your-model-name') {
      demoOutput.innerHTML = '<span class="demo-placeholder">Enter a model name above to try the live demo.</span>';
      return;
    }
    const prompt = demoPrompt.value.trim();
    if (!prompt) return;

    const streaming = demoStreamToggle.checked;
    demoAbort = new AbortController();
    demoSend.disabled = true;
    demoSend.style.display = 'none';
    demoStop.style.display = '';
    demoOutput.innerHTML = '<div class="demo-loading"><div class="demo-spinner"></div><span>' + (streaming ? 'Connecting to stream\u2026' : 'Sending request\u2026') + '</span></div>';
    demoOutput.classList.toggle('streaming', streaming);
    demoStatus.textContent = '';
    const t0 = performance.now();

    const req = buildDemoRequest(format, url, key, model, prompt, streaming);

    try {
      const res = await fetch(req.endpoint, {
        method: 'POST',
        headers: req.headers,
        body: JSON.stringify(req.body),
        signal: demoAbort.signal,
      });

      if (!res.ok) {
        const err = await res.text().catch(() => 'Unknown error');
        const hint = res.status === 401 ? 'Check your API key and try again.'
          : res.status === 404 ? 'Check your server URL \u2014 endpoint not found.'
          : res.status === 503 ? 'No workers available. Ensure a worker is connected.'
          : '';
        demoOutput.innerHTML = '<div class="demo-error"><span class="demo-error-title">HTTP ' + res.status + ' Error</span>' + escHtml(err.substring(0, 200)) + (hint ? '<br><span class="demo-error-detail">' + hint + '</span>' : '') + '</div>';
        demoStatus.textContent = 'Error \u00b7 ' + Math.round(performance.now() - t0) + 'ms';
        demoOutput.classList.remove('streaming');
        return;
      }

      if (!streaming) {
        const data = await res.json();
        const content = extractNonStreamContent(format, data);
        demoOutput.textContent = content;
        const ms = Math.round(performance.now() - t0);
        demoStatus.textContent = 'Done \u00b7 ' + ms + 'ms';
        demoOutput.classList.remove('streaming');
        return;
      }

      // SSE streaming
      demoOutput.textContent = '';
      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buf = '';
      let tokens = 0;

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buf += decoder.decode(value, { stream: true });
        const lines = buf.split('\n');
        buf = lines.pop() || '';
        for (const line of lines) {
          const delta = extractStreamDelta(format, line);
          if (delta) {
            tokens++;
            const cursor = demoOutput.querySelector('.demo-cursor');
            if (cursor) cursor.remove();
            demoOutput.appendChild(document.createTextNode(delta));
            const c = document.createElement('span');
            c.className = 'demo-cursor';
            demoOutput.appendChild(c);
            demoOutput.scrollTop = demoOutput.scrollHeight;
          }
        }
        const ms = Math.round(performance.now() - t0);
        demoStatus.textContent = tokens + ' chunks \u00b7 ' + ms + 'ms';
      }
      const cursor = demoOutput.querySelector('.demo-cursor');
      if (cursor) cursor.remove();
      demoOutput.classList.remove('streaming');
      const ms = Math.round(performance.now() - t0);
      demoStatus.textContent = 'Done \u00b7 ' + tokens + ' chunks \u00b7 ' + ms + 'ms';
    } catch (e) {
      demoOutput.classList.remove('streaming');
      if (e.name === 'AbortError') {
        demoStatus.textContent = 'Stopped \u00b7 ' + Math.round(performance.now() - t0) + 'ms';
        const cursor = demoOutput.querySelector('.demo-cursor');
        if (cursor) cursor.remove();
      } else if (e.name === 'TypeError' && e.message.includes('Failed to fetch')) {
        demoOutput.innerHTML = '<div class="demo-error"><span class="demo-error-title">Connection Failed</span>Could not reach the server.<br><span class="demo-error-detail">This is usually a CORS issue or the server is unreachable. Check the URL and try again.</span></div>';
        demoStatus.textContent = 'Connection failed';
      } else {
        demoOutput.innerHTML = '<div class="demo-error"><span class="demo-error-title">Error</span>' + escHtml(e.message) + '</div>';
        demoStatus.textContent = 'Error';
      }
    } finally {
      demoSend.disabled = false;
      demoSend.style.display = '';
      demoStop.style.display = 'none';
      demoAbort = null;
    }
  }
})();
    "#;

    // ── Snippet templates (using SERVER_URL / API_KEY / MODEL_NAME placeholders) ──

    let snippet_pi = r"{
  &quot;providers&quot;: {
    &quot;modelrelay&quot;: {
      &quot;baseUrl&quot;: &quot;SERVER_URL/v1&quot;,
      &quot;api&quot;: &quot;openai-completions&quot;,
      &quot;apiKey&quot;: &quot;API_KEY&quot;,
      &quot;compat&quot;: { &quot;supportsDeveloperRole&quot;: false, &quot;supportsReasoningEffort&quot;: false },
      &quot;models&quot;: [{
        &quot;id&quot;: &quot;MODEL_NAME&quot;,
        &quot;name&quot;: &quot;My Model via ModelRelay&quot;,
        &quot;input&quot;: [&quot;text&quot;],
        &quot;contextWindow&quot;: 200000,
        &quot;maxTokens&quot;: 16384
      }]
    }
  }
}";

    let snippet_codex = r"model_provider = &quot;modelrelay&quot;
model = &quot;MODEL_NAME&quot;

[model_providers.modelrelay]
name = &quot;ModelRelay&quot;
base_url = &quot;SERVER_URL/v1&quot;
env_key = &quot;MODELRELAY_API_KEY&quot;";

    let snippet_codex_env = r"export MODELRELAY_API_KEY=API_KEY";

    let snippet_aider = r"export OPENAI_API_BASE=SERVER_URL/v1
export OPENAI_API_KEY=API_KEY
aider --model openai/MODEL_NAME";

    let snippet_continue = r"models:
  - name: My Model via ModelRelay
    provider: openai
    model: MODEL_NAME
    apiBase: SERVER_URL/v1
    apiKey: API_KEY";

    let snippet_curl = r"curl SERVER_URL/v1/chat/completions \
  -H &quot;Content-Type: application/json&quot; \
  -H &quot;Authorization: Bearer API_KEY&quot; \
  -d &#x27;{
    &quot;model&quot;: &quot;MODEL_NAME&quot;,
    &quot;messages&quot;: [{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}]
  }&#x27;";

    let snippet_python = r"from openai import OpenAI

client = OpenAI(
    base_url=&quot;SERVER_URL/v1&quot;,
    api_key=&quot;API_KEY&quot;,
)

response = client.chat.completions.create(
    model=&quot;MODEL_NAME&quot;,
    messages=[{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}],
)
print(response.choices[0].message.content)";

    let snippet_node = r"import OpenAI from &quot;openai&quot;;

const client = new OpenAI({
  baseURL: &quot;SERVER_URL/v1&quot;,
  apiKey: &quot;API_KEY&quot;,
});

const response = await client.chat.completions.create({
  model: &quot;MODEL_NAME&quot;,
  messages: [{ role: &quot;user&quot;, content: &quot;Hello!&quot; }],
});
console.log(response.choices[0].message.content);";

    let snippet_go = r"package main

import (
    &quot;context&quot;
    &quot;fmt&quot;
    openai &quot;github.com/sashabaranov/go-openai&quot;
)

func main() {
    cfg := openai.DefaultConfig(&quot;API_KEY&quot;)
    cfg.BaseURL = &quot;SERVER_URL/v1&quot;
    client := openai.NewClientWithConfig(cfg)

    resp, _ := client.CreateChatCompletion(context.Background(),
        openai.ChatCompletionRequest{
            Model: &quot;MODEL_NAME&quot;,
            Messages: []openai.ChatCompletionMessage{
                {Role: &quot;user&quot;, Content: &quot;Hello!&quot;},
            },
        },
    )
    fmt.Println(resp.Choices[0].Message.Content)
}";

    // ── Anthropic Messages API snippet templates ──

    let snippet_anthropic_curl = r"curl SERVER_URL/v1/messages \
  -H &quot;Content-Type: application/json&quot; \
  -H &quot;x-api-key: API_KEY&quot; \
  -H &quot;anthropic-version: 2023-06-01&quot; \
  -d &#x27;{
    &quot;model&quot;: &quot;MODEL_NAME&quot;,
    &quot;max_tokens&quot;: 1024,
    &quot;messages&quot;: [{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}]
  }&#x27;";

    let snippet_anthropic_python = r"from anthropic import Anthropic

client = Anthropic(
    base_url=&quot;SERVER_URL/v1&quot;,
    api_key=&quot;API_KEY&quot;,
)

message = client.messages.create(
    model=&quot;MODEL_NAME&quot;,
    max_tokens=1024,
    messages=[{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}],
)
print(message.content[0].text)";

    let snippet_anthropic_curl_stream = r"curl -N SERVER_URL/v1/messages \
  -H &quot;Content-Type: application/json&quot; \
  -H &quot;x-api-key: API_KEY&quot; \
  -H &quot;anthropic-version: 2023-06-01&quot; \
  -d &#x27;{
    &quot;model&quot;: &quot;MODEL_NAME&quot;,
    &quot;max_tokens&quot;: 1024,
    &quot;stream&quot;: true,
    &quot;messages&quot;: [{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}]
  }&#x27;";

    let snippet_anthropic_python_stream = r"from anthropic import Anthropic

client = Anthropic(
    base_url=&quot;SERVER_URL/v1&quot;,
    api_key=&quot;API_KEY&quot;,
)

with client.messages.stream(
    model=&quot;MODEL_NAME&quot;,
    max_tokens=1024,
    messages=[{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}],
) as stream:
    for text in stream.text_stream:
        print(text, end=&quot;&quot;, flush=True)
print()";

    // ── OpenAI Responses API snippet templates ──

    let snippet_responses_curl = r"curl SERVER_URL/v1/responses \
  -H &quot;Content-Type: application/json&quot; \
  -H &quot;Authorization: Bearer API_KEY&quot; \
  -d &#x27;{
    &quot;model&quot;: &quot;MODEL_NAME&quot;,
    &quot;input&quot;: &quot;Hello!&quot;
  }&#x27;";

    let snippet_responses_python = r"from openai import OpenAI

client = OpenAI(
    base_url=&quot;SERVER_URL/v1&quot;,
    api_key=&quot;API_KEY&quot;,
)

response = client.responses.create(
    model=&quot;MODEL_NAME&quot;,
    input=&quot;Hello!&quot;,
)
print(response.output_text)";

    let snippet_responses_curl_stream = r"curl -N SERVER_URL/v1/responses \
  -H &quot;Content-Type: application/json&quot; \
  -H &quot;Authorization: Bearer API_KEY&quot; \
  -d &#x27;{
    &quot;model&quot;: &quot;MODEL_NAME&quot;,
    &quot;input&quot;: &quot;Hello!&quot;,
    &quot;stream&quot;: true
  }&#x27;";

    let snippet_responses_python_stream = r"from openai import OpenAI

client = OpenAI(
    base_url=&quot;SERVER_URL/v1&quot;,
    api_key=&quot;API_KEY&quot;,
)

stream = client.responses.create(
    model=&quot;MODEL_NAME&quot;,
    input=&quot;Hello!&quot;,
    stream=True,
)
for event in stream:
    if event.type == &quot;response.output_text.delta&quot;:
        print(event.delta, end=&quot;&quot;, flush=True)
print()";

    // ── Streaming snippet templates ──

    let snippet_curl_stream = r"curl -N SERVER_URL/v1/chat/completions \
  -H &quot;Content-Type: application/json&quot; \
  -H &quot;Authorization: Bearer API_KEY&quot; \
  -d &#x27;{
    &quot;model&quot;: &quot;MODEL_NAME&quot;,
    &quot;stream&quot;: true,
    &quot;messages&quot;: [{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}]
  }&#x27;";

    let snippet_python_stream = r"from openai import OpenAI

client = OpenAI(
    base_url=&quot;SERVER_URL/v1&quot;,
    api_key=&quot;API_KEY&quot;,
)

stream = client.chat.completions.create(
    model=&quot;MODEL_NAME&quot;,
    messages=[{&quot;role&quot;: &quot;user&quot;, &quot;content&quot;: &quot;Hello!&quot;}],
    stream=True,
)
for chunk in stream:
    delta = chunk.choices[0].delta.content
    if delta:
        print(delta, end=&quot;&quot;, flush=True)
print()";

    let snippet_node_stream = r"import OpenAI from &quot;openai&quot;;

const client = new OpenAI({
  baseURL: &quot;SERVER_URL/v1&quot;,
  apiKey: &quot;API_KEY&quot;,
});

const stream = await client.chat.completions.create({
  model: &quot;MODEL_NAME&quot;,
  messages: [{ role: &quot;user&quot;, content: &quot;Hello!&quot; }],
  stream: true,
});
for await (const chunk of stream) {
  const delta = chunk.choices?.[0]?.delta?.content;
  if (delta) process.stdout.write(delta);
}
console.log();";

    let snippet_go_stream = r"package main

import (
    &quot;context&quot;
    &quot;fmt&quot;
    &quot;io&quot;
    openai &quot;github.com/sashabaranov/go-openai&quot;
)

func main() {
    cfg := openai.DefaultConfig(&quot;API_KEY&quot;)
    cfg.BaseURL = &quot;SERVER_URL/v1&quot;
    client := openai.NewClientWithConfig(cfg)

    stream, _ := client.CreateChatCompletionStream(
        context.Background(),
        openai.ChatCompletionRequest{
            Model: &quot;MODEL_NAME&quot;,
            Messages: []openai.ChatCompletionMessage{
                {Role: &quot;user&quot;, Content: &quot;Hello!&quot;},
            },
        },
    )
    defer stream.Close()
    for {
        resp, err := stream.Recv()
        if err == io.EOF { break }
        if err != nil { break }
        fmt.Print(resp.Choices[0].Delta.Content)
    }
    fmt.Println()
}";

    let logged_in = cloud_config.is_some();
    let integrate_override_css = r"
    .content { padding: 32px 0; }
    .content h1 { font-size: 1.75rem; margin-bottom: 4px; }
    .subtitle { color: #8b949e; margin-bottom: 24px; font-size: 0.95rem; }
    code { font-family: 'SFMono-Regular', Consolas, monospace; }
    ";

    let extra_css = ["<style>", integrate_override_css, integrate_css, "</style>"].concat();

    let integrate_body = format!(
        r