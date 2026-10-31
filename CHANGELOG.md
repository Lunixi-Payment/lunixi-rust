# Changelog

## 0.1.1 — 2026-10-31

- Use equivalent missing-or-empty validation predicates accepted by both the Rust1.89 baseline and current Clippy, without suppressing warnings. Payment behavior and dependencies are unchanged.

## 0.1.0 — 2026-10-31

- Initial asynchronous Rust client with shared signed HTTPS transport and concurrent token cache.
- JavaScript-compatible canonical JSON, Ed25519 PKCS8/seed authentication and strict integer money.
- Typed payment lifecycle/fraud APIs, split payment validation and generic documented merchant requests.
- Stable keyed bounded retries, disabled redirects and secret-safe diagnostics.
- Strict raw v2 webhook verification with rotation/freshness and optional transactional SQLite inbox.
- Runnable TEST checkout/durable receipt examples, cross-language/security/TLS/concurrency tests, English/Turkish documentation and CI.
