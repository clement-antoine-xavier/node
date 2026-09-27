# node

A decentralized database node, built in Rust on [tokio](https://docs.rs/tokio),
[tonic](https://docs.rs/tonic) (gRPC, node-to-node),
[hyper](https://docs.rs/hyper) (HTTP, node-to-client),
[tower](https://docs.rs/tower) (middleware), and
[tracing](https://docs.rs/tracing) (logging).

At this stage the node is **transport-only**: it speaks to peers and clients,
logs, and shuts down cleanly. There is no consensus, replication, or storage
yet. The point of this milestone is a solid networking and observability
foundation to build the database on.

## Layout

```
crates/
  proto/   Protobuf definitions + tonic code generation
  net/     Networking library (p2p gRPC, client HTTP, middleware, config, error)
  node/    The `node` binary: CLI, config loading, logging, app wiring
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

- `crates/net/tests/p2p.rs` — gRPC round-trips: ping/status, handshake, and the
  client-side method filter.
- `crates/net/tests/client.rs` — HTTP surface: `/health`, `/status`, `404`, `405`.
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

### Node-to-client (HTTP, hyper)

- `GET /health` — liveness
- `GET /status` — node and network summary
- `GET /v1/peers` — connected peers (placeholder)

The server stack (`crates/net/src/client/server.rs`) is:

```
trace -> timeout -> body limit -> concurrency limit -> rate limit
      -> method filter -> router
```

Unknown routes return `404`; disallowed methods return `405`.

## Configuration

See `config.example.toml`. Each field is optional and can be overridden by
flags or environment variables (`--help` lists them):

| Setting | Flag | Env |
| --- | --- | --- |
| Config file | `--config` | `NODE_CONFIG` |
| Node id | `--node-id` | `NODE_ID` |
| P2P listen | `--p2p-listen` | `NODE_P2P_LISTEN` |
| Advertised address | `--p2p-advertise` | `NODE_P2P_ADVERTISE` |
| Client listen | `--client-listen` | `NODE_CLIENT_LISTEN` |
| Bootstrap peers | `--peers` | `NODE_PEERS` |
| Log filter | `--log` | `NODE_LOG` |

## Shutdown

SIGINT/SIGTERM cancel a shared `CancellationToken`; the gRPC and HTTP servers
drain and stop, and the process exits cleanly.
