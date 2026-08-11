
use modelrelay_web::integrate_renderer::render_integrate_page;

#[test]
fn integrate_page_renders_manifest_content() {
    let html = render_integrate_page();
    assert!(html.contains("Integrate"));
    assert!(html.contains("Last verified"));
    assert!(html.contains("int-server-url"));
    assert!(html.contains("int-api-key"));
    assert!(html.contains("int-model-name"));
    // Ensure no inline style/script
    assert!(!html.contains("<style>"));
    assert!(!html.contains("<script>"));
}
