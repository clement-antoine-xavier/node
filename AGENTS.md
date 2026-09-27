# AGENTS.md

Rust workspace for a decentralized-database node. Currently a **transport-only
scaffold**: gRPC (node↔node), HTTP (node↔client), Ed25519 message signing,
tracing, graceful shutdown. There is **no consensus, storage, or database logic
yet** — don't invent it, and keep new work behind a clear interface.

## Crates (dependency direction: `proto` → `identity` → `net` → `node`)

- `crates/proto` — `.proto` files + tonic code generation only.
- `crates/identity` — Ed25519 keypairs, signatures, key-file persistence.
- `crates/net` — networking library: p2p gRPC, client HTTP, `tower` stacks, auth, config, errors.
- `crates/node` — **both** a lib (`node::app::App`, `config`, `cli`, `telemetry`, `sign`) and the `node` binary (`main.rs` is thin). Integration tests drive the lib.
- Root `Cargo.toml` is a pure workspace (no root package); members live in `crates/`.

## Commands

- CI order, run before pushing: `cargo fmt --all -- --check` → `cargo clippy --all-targets -- -D warnings` → `cargo build` → `cargo test`.
- Single package / test: `cargo test -p net --test p2p`, `cargo test -p node --test e2e`, `cargo test ping_and_status`.
- Live two-node check: `./scripts/smoke.sh` (bash; fixed ports 9101/9102 and 8181/8182).

## Toolchain / build quirks

- Edition 2024, `resolver = "3"`; Rust 1.85+ (developed on 1.98). Let-chains are used.
- **protoc is vendored** — `crates/proto/build.rs` uses `protoc-bin-vendored` unless `PROTOC` is already set. No system install needed; editing a `.proto` triggers regen.
- tonic 0.14 split prost out: codegen uses `tonic-prost-build` and generated code needs the `tonic-prost` dependency. Don't "fix" this to `tonic-build`.
- Workspace `tokio` is declared with `default-features = false`. When adding tokio to a crate, use `tokio = { workspace = true, features = [...] }` and list every feature you need (`rt`/`net`/`time`/`macros`/`sync`/`signal`/`io-util`); missing features produce confusing compile errors.

## Gotchas that cost real time

- `tower::discover::Discover` is sealed — use `tower::discover::ServiceList`, not a custom impl.
- `tower::limit::RateLimit` is **not** `Clone`; a `tower::buffer::Buffer` at the stack edge makes a stack cloneable.
- `tower::util::BoxCloneService` is `Send` but **not** `Sync`; `PeerPool` wraps it in `Arc<Mutex<..>>` so futures borrowing it stay `Send`.
- tonic 0.14 server: `add_service` returns a `Router`; `with_interceptor` returns an `InterceptedService` around the *inner* service, so set `max_decoding_message_size` on `NodeServiceServer::new(..)` **before** wrapping (see `crates/net/src/p2p/server.rs`).
- The generated server trait is `#[async_trait]`; the client service body type is `tonic::body::Body`.

## Message signing

- Everything is signed. Scheme lives in `crates/net/src/auth.rs`: canonical length-prefixed payload + domain separation + timestamp with a clock-skew window.
- Peer signatures travel in gRPC metadata (`x-peer-*`); client signatures in HTTP headers (`x-client-*`).
- Client HTTP: `/health` is open; every other route requires a valid signature or returns `401`. Generate headers with `node sign --key-file FILE --method GET --path /status`.
- Node key persists as hex in `identity.key_file` (default `node.key`; gitignored; `0600` on Unix) and auto-generates on first run.
- Tests/curl hitting protected routes unsigned will get `401` **by design** — pass signatures.

## Config

- Precedence: TOML file → env → CLI flag (`cargo run -p node -- --help` lists env vars). `RUST_LOG` overrides `log.level`.
- When adding a tunable, update both `crates/net/src/config.rs` (`P2pConfig`/`ClientConfig`) and `config.example.toml`.

## Tests

- Integration tests bind ephemeral ports and start real servers; no external services required.
- `crates/node/tests/e2e.rs` boots two full nodes through `node::app::App` and writes throwaway key files under the temp dir.
