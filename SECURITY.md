# Security policy

Report suspected vulnerabilities privately to **developer@lunixi.com**. Include the affected SDK version, a synthetic reproduction, impact and suggested mitigation. Do not put customer data, credentials or live exploits in public issues. The latest 0.1.x patch receives fixes; use supported patched Rust and dependency versions. We investigate and coordinate disclosure after a fix is available, without promising a response deadline.

Keep private keys/webhook secrets on the server. Use separate TEST/LIVE credentials, trusted deployment URLs and least-privilege merchant scopes. Debug output redacts credentials and response bodies; explicit data/raw accessors may expose sensitive values. Do not log them. Zeroizing buffers reduce retained secret lifetimes but do not guarantee that all temporary application/runtime copies are erased.

TLS certificate/hostname verification remains enabled. Redirects, implicit retries and environment proxies are disabled. Only an explicit loopback test option allows HTTP. Never expose the test option or BaseURL to customer input. Additional CA certificates must be controlled deployment assets.

Use canonical minor-unit strings and persisted per-attempt idempotency keys. Match authenticated final payment state, merchant/order/payment ID, expected amount and currency before fulfillment. A browser return, token, authorization or HTTP 200 is not capture evidence. Unknown/reconciliation/compensating states fail closed.

Authenticate exact raw webhook bytes before JSON middleware; retain duplicate identity headers for rejection. Freshness and signature checks do not consume an event. Replay marker, order transition and outbox rows must commit in the same durable database transaction. Return a retryable failure if persistence fails. Retain markers for your replay policy. External effects belong in an outbox worker. The receipt example stores notifications only and does not implement fulfillment.

Run formatting, all-feature tests/clippy, cargo-audit and cargo-deny. Test credentials are synthetic public fixtures. Direct raw-card collection is not implemented in the default examples. This SDK does not remove payment/privacy obligations.

Primary references: [RustCrypto Ed25519](https://docs.rs/ed25519-dalek/3.0.0/ed25519_dalek/), [RustCrypto HMAC verification](https://docs.rs/hmac/0.13.0/hmac/), [reqwest security configuration](https://docs.rs/reqwest/0.13.5/reqwest/struct.ClientBuilder.html), [RustSec](https://rustsec.org/).
