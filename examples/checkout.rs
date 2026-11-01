//! Creates a TEST intent from trusted order values; no raw cards are collected.
use lunixi::{BasketItem, Client, Config, Error, IntentRequest};
#[path = "support/checkout_presentation.rs"]
mod presentation;

#[tokio::main]
async fn main() -> lunixi::Result<()> {
    let key_id = std::env::var("LUNIXI_KEY_ID").map_err(|_| Error::Invalid("LUNIXI_KEY_ID"))?;
    if !key_id.starts_with("mk_test_") {
        return Err(Error::Invalid("TEST key required by example"));
    }
    let order = std::env::var("LUNIXI_ORDER_ID")
        .map_err(|_| Error::Invalid("persisted LUNIXI_ORDER_ID"))?;
    let attempt = std::env::var("LUNIXI_ATTEMPT_KEY")
        .map_err(|_| Error::Invalid("persisted LUNIXI_ATTEMPT_KEY"))?;
    let client = Client::new(Config::new(
        std::env::var("LUNIXI_BASE_URL").map_err(|_| Error::Invalid("LUNIXI_BASE_URL"))?,
        key_id,
        std::env::var("LUNIXI_PRIVATE_KEY_PEM")
            .map_err(|_| Error::Invalid("LUNIXI_PRIVATE_KEY_PEM"))?,
    ))?;
    let mut request = IntentRequest::new(
        "10050",
        "TRY",
        order,
        "https://merchant.example/payment/return",
    );
    request.basket_items.push(BasketItem::new(
        "line-1",
        "Example product",
        "PHYSICAL",
        "10050",
    ));
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(35),
        client.payments().create_intent(&request, &attempt),
    )
    .await
    .map_err(|_| Error::Transport)??;
    let data = response
        .data
        .as_ref()
        .ok_or(Error::Response("checkout result"))?;
    if data
        .get("paymentId")
        .and_then(|id| id.as_str())
        .is_none_or(str::is_empty)
    {
        return Err(Error::Response("paymentId"));
    }
    let token = data
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or(Error::Response("checkout token"))?;
    match std::env::var("LUNIXI_CHECKOUT_MODE")
        .as_deref()
        .unwrap_or("embedded")
    {
        "embedded" => {
            let _html = presentation::render_checkout_html(token, None)?;
        }
        "hosted" => {
            let _url = presentation::hosted_checkout_url(
                data.get("paymentPageUrl")
                    .and_then(|u| u.as_str())
                    .unwrap_or(""),
            )?;
        }
        _ => return Err(Error::Invalid("checkout mode")),
    }
    // Your server should send the token only to this order's authenticated
    // customer with Cache-Control: no-store, then use the existing Lunixi form.
    // Keep the token out of logs, browser URLs, analytics and error reports.
    println!("TEST checkout ready; token is intentionally omitted from console output");
    Ok(())
}
