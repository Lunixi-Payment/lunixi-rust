# Contributing

Describe substantial feature proposals in an issue first. Send vulnerability reports through [SECURITY.md](SECURITY.md).

Use Rust 1.89+ and preserve the shared authentication/transport/money/webhook implementation. Framework examples must reuse the SDK. Implement only verified public endpoints and preserve envelopes, pending states and reconciliation requirements. Avoid raw-card examples and customer/secret logging. Keep the original Lunixi logo.

Run `cargo fmt --all --check`, `cargo test --locked --all-targets --all-features`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --doc --all-features`, `cargo audit`, `cargo deny --all-features check advisories licenses sources` and `cargo package --locked --allow-dirty --all-features`. Also test the default feature set. Do not commit target output, credentials, customer data or database files. Use synthetic offline regression tests for security/financial changes.

Update the English README, Turkish documentation, changelog and examples for public API changes. Explain the concrete trigger, resulting behavior and actual verification in pull requests. Publishing a crate/tag is a separate maintainer release action; this repository does not automatically publish to crates.io.
