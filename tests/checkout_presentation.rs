#[path = "../examples/support/checkout_presentation.rs"]
mod presentation;

#[test]
fn merchant_form_uses_escaped_object_and_existing_sdk() -> Result<(), Box<dyn std::error::Error>> {
    let token = "</script><svg/onload=unexpected()>";
    let html = presentation::render_checkout_html(token, Some("server_nonce_123456789"))?;
    assert!(!html.contains(token));
    assert!(html.contains("https://cdn.lunixi.com/lunixi-checkout.min.js"));
    assert!(html.contains("checkout.mount('#lunixi-checkout-form')"));
    let start = html
        .find("id=\"lunixi-checkout-config\"")
        .ok_or("missing checkout config")?;
    let body = &html[start..];
    let body = &body[body.find('>').ok_or("missing config tag")? + 1
        ..body.find("</script>").ok_or("missing config end")?];
    let config: serde_json::Value = serde_json::from_str(body)?;
    assert_eq!(config["type"], "payment");
    assert_eq!(config["options"]["token"], token);
    assert!(presentation::render_checkout_html("token", Some("bad\"nonce")).is_err());
    Ok(())
}

#[test]
fn optional_hosted_uses_independent_backend_url_and_rejects_unsafe_targets()
-> Result<(), Box<dyn std::error::Error>> {
    let url = "https://pay.lunixi.com/payment/independent-hosted-token-12345";
    assert_eq!(presentation::hosted_checkout_url(url)?, url);
    for value in [
        "",
        "http://pay.lunixi.com/payment/opaque-hosted-token-1234",
        "https://evil.example/payment/opaque-hosted-token-1234",
        "https://pay.lunixi.com:443/payment/opaque-hosted-token-1234",
        "https://user@pay.lunixi.com/payment/opaque-hosted-token-1234",
        "https://pay.lunixi.com/payment/opaque-hosted-token-1234?x=1",
        "https://pay.lunixi.com/payment/opaque-hosted-token-1234#x",
        "https://pay.lunixi.com/payment/opaque-hosted-token-1234\n",
        "https://pay.lunixi.com/payment/short",
        "https://pay.lunixi.com/payment/escaped%2Ftoken-12345",
    ] {
        assert!(presentation::hosted_checkout_url(value).is_err());
    }
    Ok(())
}
