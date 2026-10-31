#![cfg(feature = "sqlite")]
#![allow(clippy::unwrap_used, clippy::expect_used)]
use hmac::{Hmac, KeyInit, Mac};
use lunixi::{Error, SqliteInbox, WebhookEvent, WebhookVerifier};
use reqwest::header::{HeaderMap, HeaderValue};
use rusqlite::Connection;
use sha2::Sha256;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;

struct TempDb(std::path::PathBuf);
impl TempDb {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("lunixi-inbox-{}.db", Uuid::new_v4())))
    }
    fn open(&self) -> Connection {
        let c = Connection::open(&self.0).unwrap();
        c.busy_timeout(Duration::from_secs(5)).unwrap();
        c
    }
}
impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn event(id: &str, typ: &str) -> WebhookEvent {
    let secret = "synthetic_webhook_test_secret_123456";
    let ts = chrono::Utc::now().to_rfc3339();
    let raw = format!(
        r#"{{"type":"{typ}","data":{{"orderId":"order-1","capturedAmountMinor":"10050","currency":"TRY"}}}}"#
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{id}.{ts}.{raw}").as_bytes());
    let hex = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut h = HeaderMap::new();
    for (k, v) in [
        ("x-lunixi-event-id", id),
        ("x-lunixi-signature-timestamp", &ts),
        ("x-lunixi-signature", &format!("v2={hex}")),
    ] {
        h.insert(k, HeaderValue::from_str(v).unwrap());
    }
    WebhookVerifier::new(vec![secret.into()])
        .unwrap()
        .verify(raw.as_bytes(), &h)
        .unwrap()
}
#[test]
fn rollback_retry_duplicate_collision_and_restart_are_atomic() {
    let file = TempDb::new();
    let mut db = file.open();
    SqliteInbox::ensure_schema(&db).unwrap();
    let inbox = SqliteInbox::new("merchant-a").unwrap();
    db.execute_batch("CREATE TABLE orders (id TEXT PRIMARY KEY,applications INTEGER NOT NULL);INSERT INTO orders VALUES ('order-1',0)").unwrap();
    let ev = event("event-1", "payment.captured");
    assert_eq!(
        inbox.process(&mut db, &ev, |tx| {
            tx.execute("UPDATE orders SET applications=applications+1", [])
                .unwrap();
            Err(Error::Invalid("synthetic crash"))
        }),
        Err(Error::Invalid("synthetic crash"))
    );
    let apply = |tx: &rusqlite::Transaction<'_>| {
        tx.execute("UPDATE orders SET applications=applications+1", [])
            .map_err(|_| Error::Inbox)?;
        Ok(())
    };
    assert!(inbox.process(&mut db, &ev, apply).unwrap());
    assert!(!inbox.process(&mut db, &ev, apply).unwrap());
    let count: i64 = db
        .query_row("SELECT applications FROM orders", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    drop(db);
    let mut db = file.open();
    assert!(!inbox.process(&mut db, &ev, apply).unwrap());
    assert_eq!(
        inbox.process(&mut db, &event("event-1", "payment.failed"), apply),
        Err(Error::Webhook)
    );
    assert!(
        SqliteInbox::new("merchant-b")
            .unwrap()
            .process(&mut db, &ev, |_| Ok(()))
            .unwrap()
    );
}
#[test]
fn concurrent_processes_use_the_persistent_unique_event_marker() {
    let file = Arc::new(TempDb::new());
    let db = file.open();
    SqliteInbox::ensure_schema(&db).unwrap();
    drop(db);
    let ev = Arc::new(event("event-concurrent", "payment.captured"));
    let commits = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];
    for _ in 0..12 {
        let file = file.clone();
        let ev = ev.clone();
        let commits = commits.clone();
        handles.push(std::thread::spawn(move || {
            let mut db = file.open();
            let inbox = SqliteInbox::new("merchant-a").unwrap();
            if inbox.process(&mut db, &ev, |_| Ok(())).unwrap() {
                commits.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }
    for h in handles {
        h.join().unwrap()
    }
    assert_eq!(commits.load(Ordering::SeqCst), 1);
}
#[test]
fn failed_database_does_not_acknowledge_event() {
    let mut db = Connection::open_in_memory().unwrap();
    let inbox = SqliteInbox::new("merchant-a").unwrap();
    let ev = event("event-dbfail", "payment.captured");
    assert_eq!(inbox.process(&mut db, &ev, |_| Ok(())), Err(Error::Inbox));
    SqliteInbox::ensure_schema(&db).unwrap();
    assert!(inbox.process(&mut db, &ev, |_| Ok(())).unwrap());
}
