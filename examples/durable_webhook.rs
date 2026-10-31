//! A runnable durable receipt consumer. Feed an authentic raw webhook on stdin
//! and its delivery headers through environment variables. It records only
//! authenticated event IDs/types, and deliberately does not fulfill orders.
use lunixi::{Error, SqliteInbox, WebhookVerifier};
use reqwest::header::{HeaderMap, HeaderValue};
use rusqlite::{Connection, params};
use std::io::Read;

fn main() -> lunixi::Result<()> {
    let secret = std::env::var("LUNIXI_WEBHOOK_SECRET")
        .map_err(|_| Error::Invalid("LUNIXI_WEBHOOK_SECRET"))?;
    let verifier = WebhookVerifier::new(vec![secret])?;
    let mut headers = HeaderMap::new();
    for (name, env) in [
        ("x-lunixi-event-id", "LUNIXI_EVENT_ID"),
        ("x-lunixi-signature-timestamp", "LUNIXI_SIGNATURE_TIMESTAMP"),
        ("x-lunixi-signature", "LUNIXI_SIGNATURE"),
    ] {
        let value = std::env::var(env).map_err(|_| Error::Invalid("webhook delivery header"))?;
        headers.insert(
            name,
            HeaderValue::from_str(&value).map_err(|_| Error::Webhook)?,
        );
    }
    let mut raw = vec![];
    std::io::stdin()
        .take((8 << 20) + 1)
        .read_to_end(&mut raw)
        .map_err(|_| Error::Webhook)?;
    let event = verifier.verify(&raw, &headers)?;
    let mut db = Connection::open("lunixi-webhooks.db").map_err(|_| Error::Inbox)?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| Error::Inbox)?;
    SqliteInbox::ensure_schema(&db)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS payment_notifications (event_id TEXT PRIMARY KEY,event_type TEXT NOT NULL)").map_err(|_|Error::Inbox)?;
    let inbox = SqliteInbox::new("merchant-payments")?;
    let applied = inbox.process(&mut db, &event, |tx| {
        // Business state and outbox writes belong in this same transaction.
        // Validate merchant/order/payment/expected amount/currency/final state
        // before changing an order; never use the browser return as authority.
        tx.execute(
            "INSERT INTO payment_notifications (event_id,event_type) VALUES (?1,?2)",
            params![event.id(), event.event_type()],
        )
        .map_err(|_| Error::Inbox)?;
        Ok(())
    })?;
    println!("Verified receipt committed: {applied}");
    Ok(())
}
