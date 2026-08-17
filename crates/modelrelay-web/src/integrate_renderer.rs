
use serde_json::Value;
use crate::content::INTEGRATIONS_MANIFEST_JSON;

fn html_escape(s: &str) -> String {
    s.replace("&","&amp;").replace("<","&lt;").replace(">","&gt;").replace(""","&quot;")
}

pub fn render_integrate_page() -> String {
    let v: Value = serde_json::from_str(INTEGRATIONS_MANIFEST_JSON).unwrap_or_default();
    let last_verified = v.get("last_verified").and_then(|x| x.as_str()).unwrap_or("");
    let variables = v.get("variables").and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|s| s.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut html = String::new();
    html.push_str(r#"<div class="content"><h1>Integrate</h1>"#);
    html.push_str(&format!(r#"<p class="subtitle">Last verified: {}</p>"#, html_escape(last_verified)));
    
    // Inputs
    html.push_str(r#"<div class="integrate-inputs">"#);
    for var in &variables {
        let id = var.to_lowercase();
        html.push_str(&format!(r#"<div class="field"><label>{}</label><input id="int-{}" placeholder="{}"></div>"#, html_escape(var), id, html_escape(var)));
    }
    html.push_str("</div>");

    // Categories
    if let Some(cats) = v.get("categories").and_then(|x| x.as_array()) {
        for cat in cats {
            let cat_name = cat.get("name").and_then(|x| x.as_str()).unwrap_or("");
            let items = cat.get("items").and_then(|x| x.as_array()).unwrap_or(&vec![]);
            html.push_str(&format!(r#"<h2 class="section-heading">{}</h2>"#, html_escape(cat_name)));
            html.push_str(r#"<div class="int-tabs">"#);
            for (i, item) in items.iter().enumerate() {
                let id = item.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                let active = if i==0 { " active" } else { "" };
                html.push_str(&format!(r#"<button class="tab{}" data-tab="{}">{}</button>"#, active, html_escape(id), html_escape(name)));
            }
            html.push_str("</div><div class="int-panel">");
            for (i, item) in items.iter().enumerate() {
                let id = item.get("id").and_then(|x| x.as_str()).unwrap_or("");
                let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("");
                let active = if i==0 { " active" } else { "" };
                html.push_str(&format!(r#"<div class="int-content{}" data-tab="{}"><h3>{}</h3>"#, active, html_escape(id), html_escape(name)));
                // snippet
                if let Some(snips) = v.get("snippets") {
                    if let Some(sn) = snips.get(id) {
                        let desc = sn.get("description").and_then(|x| x.as_str()).unwrap_or("");
                        let tmpl = sn.get("template").and_then(|x| x.as_str()).unwrap_or("");
                        html.push_str(&format!(r#"<p>{}</p><pre class="code-block"><code>{}</code></pre>"#, html_escape(desc), html_escape(tmpl)));
                    } else {
                        html.push_str("<p>No snippet available.</p>");
                    }
                }
                html.push_str("</div>");
            }
            html.push_str("</div>");
        }
    }
    html.push_str("</div>");
    html
}
