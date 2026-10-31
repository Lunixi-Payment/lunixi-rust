//! Merchant-page rendering helpers, not a standalone customer-authentication server.
use lunixi::{Error, Result};

pub fn render_checkout_html(token: &str, nonce: Option<&str>) -> Result<String> {
    if token.is_empty()
        || token.len() > 16_384
        || !token.bytes().all(|c| (0x21..=0x7e).contains(&c))
    {
        return Err(Error::Invalid("checkout token"));
    }
    if nonce.is_some_and(|n| {
        !(16..=128).contains(&n.len())
            || !n
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"+/_=-".contains(&c))
    }) {
        return Err(Error::Invalid("server CSP nonce"));
    }
    let config = serde_json::to_string(&serde_json::json!({"type":"payment","target":"lunixi-checkout-form","options":{"token":token}}))
        .map_err(|_| Error::Response("checkout configuration"))?
        .replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026");
    let attribute = nonce.map_or_else(String::new, |n| format!(" nonce=\"{n}\""));
    Ok(format!(
        r#"<div id="lunixi-checkout-form"></div><script src="https://cdn.lunixi.com/lunixi-checkout.min.js"{attribute}></script><script type="application/json" id="lunixi-checkout-config"{attribute}>{config}</script><script{attribute}>(()=>{{const config=JSON.parse(document.getElementById('lunixi-checkout-config').textContent);const checkout=Lunixi.checkout.create(config);checkout.mount('#lunixi-checkout-form');}})();</script>"#
    ))
}

/// Use only the backend URL, whose token is independent of the embedded token.
pub fn hosted_checkout_url(value: &str) -> Result<&str> {
    let token = value
        .strip_prefix("https://pay.lunixi.com/payment/")
        .ok_or(Error::Response(
            "backend paymentPageUrl unavailable or invalid",
        ))?;
    if !(16..=4096).contains(&token.len())
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.~-".contains(&c))
    {
        return Err(Error::Response(
            "backend paymentPageUrl unavailable or invalid",
        ));
    }
    Ok(value)
}
