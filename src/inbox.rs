use crate::{Error, Result, WebhookEvent, client::safe_header};
use rusqlite::{Connection, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};
/// Persistent SQLite replay inbox. The caller owns the connection and driver
/// configuration. Event markers and business updates share one transaction.
pub struct SqliteInbox {
    scope: String,
}
impl SqliteInbox {
    /// Uses a separate scope for each merchant/integration. Migrations create
    /// only an event ID/hash table; the raw webhook is never persisted here.
    pub fn new(scope: impl Into<String>) -> Result<Self> {
        let scope = scope.into();
        if !safe_header(&scope, 200) {
            return Err(Error::Invalid("inbox scope"));
        }
        Ok(Self { scope })
    }
    /// Applies explicit schema migration. Do this during startup/migrations.
    pub fn ensure_schema(connection: &Connection) -> Result<()> {
        connection.execute_batch("CREATE TABLE IF NOT EXISTS lunixi_webhook_inbox (scope TEXT NOT NULL,event_id TEXT NOT NULL,payload_sha256 BLOB NOT NULL,received_at TEXT NOT NULL,PRIMARY KEY(scope,event_id))").map_err(|_|Error::Inbox)
    }
    /// Applies a verified event once. A duplicate returns false. Callback or
    /// database failure rolls back its marker, permitting provider retry.
    /// The callback MUST use this transaction for financial state/outbox rows;
    /// external API calls/emails are not atomic and belong in an outbox worker.
    pub fn process<F>(
        &self,
        connection: &mut Connection,
        event: &WebhookEvent,
        apply: F,
    ) -> Result<bool>
    where
        F: FnOnce(&Transaction<'_>) -> Result<()>,
    {
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| Error::Inbox)?;
        let hash = Sha256::digest(event.raw());
        let inserted=tx.execute("INSERT INTO lunixi_webhook_inbox (scope,event_id,payload_sha256,received_at) VALUES (?1,?2,?3,?4) ON CONFLICT(scope,event_id) DO NOTHING",params![self.scope,event.id(),hash.as_slice(),chrono::Utc::now().to_rfc3339()]).map_err(|_|Error::Inbox)?;
        if inserted == 0 {
            let previous:Vec<u8>=tx.query_row("SELECT payload_sha256 FROM lunixi_webhook_inbox WHERE scope=?1 AND event_id=?2",params![self.scope,event.id()],|r|r.get(0)).map_err(|_|Error::Inbox)?;
            if previous.as_slice() != hash.as_slice() {
                return Err(Error::Webhook);
            }
            return Ok(false);
        }
        apply(&tx)?;
        tx.commit().map_err(|_| Error::Inbox)?;
        Ok(true)
    }
}
