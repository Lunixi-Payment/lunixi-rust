# Changelog

## 1.0.2 — 2026-11-01

- Preserve the exact amount guard while satisfying Clippy on the supported minimum Rust 1.89 toolchain.
- Verify minimum-toolchain lint and targeted transport checks without suppressions.

## 1.0.1 — 2026-11-01

- Apply Clippy condition simplifications, move the money test module after implementation, and propagate the test error with Result; API and money behavior are unchanged.
- Update 1.0.1 installation guidance.

## 1.0.0 — 2026-11-01

- Reject explicit null capture/refund amounts; full remaining operations use omission or the dedicated API described in MIGRATION-1.0.md.

- Breaking: public money request/response fields require canonical minor-unit strings; JSON numbers are rejected.
- Exact signed 64-bit fiat arithmetic and currency-aware decimal text conversion preserve values above JavaScript’s safe-integer limit.
- See [MIGRATION-1.0.md](MIGRATION-1.0.md) before upgrading.


## 0.1.2 - 2026-10-31

- Provide a concrete merchant-site renderer/example using the existing official Lunixi checkout form by default, with an explicit optional hosted presentation.
- Use only the validated backend paymentPageUrl for redirects; its opaque token is independent of the embedded token. Missing URL keeps embedded available and requires reconciliation for hosted mode without inventing another key.
- Preserve authentication, customer ownership/CSRF integration boundaries, no-store/token-redaction rules and raw response compatibility. Existing published tags are unchanged.

## 0.1.1 — 2026-10-31

- Use equivalent missing-or-empty validation predicates accepted by both the Rust1.89 baseline and current Clippy, without suppressing warnings. Payment behavior and dependencies are unchanged.

## 0.1.0 — 2026-10-31

- Initial asynchronous Rust client with shared signed HTTPS transport and concurrent token cache.
- JavaScript-compatible canonical JSON, Ed25519 PKCS8/seed authentication and strict integer money.
- Typed payment lifecycle/fraud APIs, split payment validation and generic documented merchant requests.
- Stable keyed bounded retries, disabled redirects and secret-safe diagnostics.
- Strict raw v2 webhook verification with rotation/freshness and optional transactional SQLite inbox.
- Runnable TEST checkout/durable receipt examples, cross-language/security/TLS/concurrency tests, English/Turkish documentation and CI.
