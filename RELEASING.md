# Releasing crypto-botters-bitbank

This is a fork of [negi-grass/crypto-botters](https://github.com/negi-grass/crypto-botters).
Keep the upstream MIT license and copyright notice in the published package.
The Rust library name is `crypto_botters`; the crates.io package name is
`crypto-botters-bitbank`.

## Before merging to main

1. Update the package version and CHANGELOG. The first fork release is 0.7.0;
   it includes generic WebSocket handler types that differ from upstream 0.6.
2. Run the checks below. Tests use fixed fixtures and a local WebSocket server;
   they do not need exchange API credentials or place orders.
3. Inspect the package file list, including LICENSE, README, examples, and tests.
4. Merge the checked commit to this fork's main branch and push it.

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features --locked
cargo check --lib --no-default-features --features bitbank,native-tls --locked
cargo check --lib --no-default-features --features bitbank,native-tls-vendored --locked
cargo check --lib --no-default-features --features bitbank,rustls-tls-native-roots --locked
cargo check --lib --no-default-features --features bitbank,rustls-tls-webpki-roots --locked
cargo package --list
cargo publish --dry-run --all-features --locked
```

CI additionally checks all exchange features with each TLS backend.
`generic-api-client` is fetched from crates.io. The historical local copy is
excluded from the workspace and does not need to be published for this release.
Examples declare required exchange features so that the packaged examples can
be checked with the requested exchange features.

## Publish

Confirm that the name `crypto-botters-bitbank` is available before the first
publish. Log into crates.io, verify your email, and authenticate locally with
`cargo login` (do not put the token into the repository).
From the clean main branch, repeat the dry run above, then:

```sh
cargo publish -p crypto-botters-bitbank --all-features --locked
```

After successful publication, tag that exact commit and push the tag:

```sh
git tag -a crypto-botters-bitbank-v0.7.0 -m 'Release crypto-botters-bitbank 0.7.0'
git push origin crypto-botters-bitbank-v0.7.0
```

Do not reuse the upstream v0.6.0 tag. Check the crate page and docs.rs build,
and verify the published crate in a fresh consumer project:

```toml
[dependencies]
crypto-botters = { package = "crypto-botters-bitbank", version = "0.7", features = ["bitbank"] }
```

Useful references: [Cargo publishing](https://doc.rust-lang.org/cargo/reference/publishing.html)
and [registry dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#multiple-locations).
