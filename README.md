# node

A decentralized database node, built in Rust on [tokio](https://docs.rs/tokio),
[tonic](https://docs.rs/tonic) (gRPC, node-to-node),
[hyper](https://docs.rs/hyper) (HTTP, node-to-client),
[tower](https://docs.rs/tower) (middleware),
[ed25519-dalek](https://docs.rs/ed25519-dalek) (message signing), and
[tracing](https://docs.rs/tracing) (logging).

At this stage the node is **transport-only**: it speaks to peers and clients,
signs and verifies every message with an Ed25519 identity, logs, and shuts down
cleanly. There is no consensus, replication, or storage yet. The point of this
milestone is a solid networking, identity and observability foundation to build
the database on.

## Layout

```
crates/
  proto/    Protobuf definitions + tonic code generation
  identity/ Ed25519 keypairs and message signing
  net/      Networking library (p2p gRPC, client HTTP, middleware, config, error)
  node/     The `node` binary: CLI, config loading, logging, app wiring
```

## Requirements

- Rust 1.85+ (edition 2024; developed against 1.98)
- No `protoc` install needed — the build uses a vendored one via
  `protoc-bin-vendored` unless `PROTOC` is already set.

## Build and test

```
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

### Tests

- `crates/net/tests/p2p.rs` — signed gRPC round-trips: ping/status, handshake,
  rejection of unsigned requests, and the client-side method filter.
- `crates/net/tests/client.rs` — HTTP surface: open `/health`, signed `/status`,
  `401` for unsigned/mis-bound signatures, `404`, `405`.
- `crates/node/tests/e2e.rs` — in-process end-to-end: boots two full nodes,
  has one handshake the other, then checks both the gRPC and HTTP surfaces and
  clean shutdown.

Run a subset with, e.g. `cargo test -p net --test p2p` or
`cargo test -p node --test e2e`.

### Live smoke test

`scripts/smoke.sh` builds the binary, starts two nodes, verifies the handshake
plus the HTTP endpoints/routing/filtering, and tears them down:

```
./scripts/smoke.sh
```

## Run

```
# Defaults: p2p on 127.0.0.1:9001, client HTTP on 127.0.0.1:8080
cargo run -p node

# With a config file and overrides
cargo run -p node -- --config config.example.toml --node-id node-a
```

A second node can dial the first:

```
cargo run -p node -- --node-id node-b \
  --p2p-listen 127.0.0.1:9002 \
  --client-listen 127.0.0.1:8081 \
  --peers http://127.0.0.1:9001
```

`node-b` will handshake and ping `node-a` on startup; `node-a` logs the inbound
handshake. Logs are human-readable by default, or JSON with `log.format = "json"`
(or `NODE_LOG=info` and a config file). `RUST_LOG` overrides the configured level.

On first run each node generates an Ed25519 key at `identity.key_file`
(`node.key` by default) and logs its `public_key`. Set `--identity-file FILE` to
place it elsewhere.

## Interfaces

### Node-to-node (gRPC, tonic)

Defined in `crates/proto/proto/node.proto` as `node.v1.NodeService`:

- `Handshake` — exchange identity and protocol metadata
- `Ping` — liveness
- `Status` — node/network snapshot

Outbound calls go through a `tower` stack (`crates/net/src/p2p/pool.rs`):

```
filter -> retry -> map_err -> timeout -> channel   (per peer)
        then power-of-two-choices load balancing across peers,
        then rate limit + concurrency limit, then a buffer
```

The server applies per-connection concurrency limiting and per-request timeouts.
Every request must carry a valid signature (see [Message signing](#message-signing));
unsigned or stale requests are rejected with `UNAUTHENTICATED` before reaching a
handler. The handshake exchanges public keys so a node can bind a peer id to the
key that signs its messages.

### Node-to-client (HTTP, hyper)

- `GET /health` — liveness (open, no signature required)
- `GET /status` — node and network summary
- `GET /v1/peers` — connected peers (placeholder)

The server stack (`crates/net/src/client/server.rs`) is:

```
trace -> timeout -> body limit -> concurrency limit -> rate limit
      -> method filter -> signature check -> router
```

Unknown routes return `404`; disallowed methods return `405`; unsigned or invalid
requests to protected routes return `401`.

## Message signing

Every node owns an Ed25519 keypair (`crates/identity`). Messages are signed over
a canonical, length-prefixed payload — `domain || field... || timestamp` — so a
recipient learns **who** sent a message and that its **contents** were not
changed. The timestamp is checked against a configurable clock-skew window to
bound replay, and the domain separates node-to-node requests, node-to-node
responses, and client requests so a signature cannot be replayed across
contexts.

Node-to-node signatures travel in gRPC metadata:

| Metadata | Meaning |
| --- | --- |
| `x-peer-public-key` | sender's public key (hex) |
| `x-peer-timestamp` | unix milliseconds |
| `x-peer-signature` | Ed25519 signature (hex) |

Requests are signed by the client (`ChannelRpc`) and verified by the server
interceptor plus handler; responses are signed by the server and verified by the
client. The handshake includes the public key, which the server checks against
the signing key.

Client-to-node signatures travel in HTTP headers:

| Header | Meaning |
| --- | --- |
| `x-client-public-key` | client's public key (hex) |
| `x-client-timestamp` | unix milliseconds |
| `x-client-signature` | Ed25519 signature (hex) |

The `sign` subcommand prints these headers for a request, so `curl` and scripts
can call protected routes:

```
node sign --key-file node.key --method GET --path /status
# x-client-public-key <hex>
# x-client-timestamp <millis>
# x-client-signature <hex>
```

Signing is always required for peer RPCs; for the client interface it is
controlled by `client.require_signature` (default `true`), with `/health` always
open.

## Configuration

See `config.example.toml`. Each field is optional and can be overridden by
flags or environment variables (`--help` lists them):

| Setting | Flag | Env |
| --- | --- | --- |
| Config file | `--config` | `NODE_CONFIG` |
| Node id | `--node-id` | `NODE_ID` |
| Identity key file | `--identity-file` | `NODE_IDENTITY_FILE` |
| P2P listen | `--p2p-listen` | `NODE_P2P_LISTEN` |
| Advertised address | `--p2p-advertise` | `NODE_P2P_ADVERTISE` |
| Client listen | `--client-listen` | `NODE_CLIENT_LISTEN` |
| Bootstrap peers | `--peers` | `NODE_PEERS` |
| Log filter | `--log` | `NODE_LOG` |

## Shutdown

SIGINT/SIGTERM cancel a shared `CancellationToken`; the gRPC and HTTP servers
drain and stop, and the process exits cleanly.
