#!/usr/bin/env bash
#
# Live smoke test: boots two nodes, verifies the signed gRPC handshake, the
# signed HTTP endpoints, routing and method filtering, then tears everything
# down.
#
# Usage: scripts/smoke.sh

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin="$root/target/debug/node"

a_p2p=9101
a_http=8181
b_p2p=9102
b_http=8182

logs="$(mktemp -d)"
key_a="$logs/node-a.key"
pid_a=""
pid_b=""

cleanup() {
  [ -n "$pid_a" ] && kill "$pid_a" 2>/dev/null || true
  [ -n "$pid_b" ] && kill "$pid_b" 2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup EXIT

wait_for() {
  local url="$1"
  for _ in $(seq 1 50); do
    curl -s -o /dev/null "$url" && return 0
    sleep 0.1
  done
  return 1
}

# Print "Name: value" signature headers using the node's own signer.
sign_headers() {
  "$bin" sign --key-file "$1" --method "$2" --path "$3" | awk '{print $1 ": " $2}'
}

# GET a path with a valid signature; mirrors what a client SDK would do.
signed_get() {
  local path="$1"
  local -a args=()
  while IFS= read -r header; do args+=(-H "$header"); done < <(sign_headers "$key_a" GET "$path")
  curl -s -o /dev/null -w '%{http_code}' "${args[@]}" "http://127.0.0.1:$a_http$path"
}

wait_for_signed_status() {
  local body=""
  for _ in $(seq 1 50); do
    local -a args=()
    while IFS= read -r header; do args+=(-H "$header"); done < <(sign_headers "$key_a" GET "/status")
    body="$(curl -s "${args[@]}" "http://127.0.0.1:$a_http/status")"
    case "$body" in *'"peer_count":1'*) echo "$body"; return 0;; esac
    sleep 0.1
  done
  echo "$body"
  return 1
}

cargo build -q -p node

echo "starting node-a (p2p $a_p2p, http $a_http)"
"$bin" --node-id node-a --identity-file "$key_a" \
  --p2p-listen "127.0.0.1:$a_p2p" --client-listen "127.0.0.1:$a_http" \
  >"$logs/a.log" 2>&1 &
pid_a=$!
wait_for "http://127.0.0.1:$a_http/health" || { echo "node-a never came up"; exit 1; }

echo "starting node-b (p2p $b_p2p, http $b_http) -> peer node-a"
"$bin" --node-id node-b --identity-file "$logs/node-b.key" \
  --p2p-listen "127.0.0.1:$b_p2p" --client-listen "127.0.0.1:$b_http" \
  --peers "http://127.0.0.1:$a_p2p" >"$logs/b.log" 2>&1 &
pid_b=$!
wait_for "http://127.0.0.1:$b_http/health" || { echo "node-b never came up"; exit 1; }

fail=0
check() {
  if [ "$2" = "$3" ]; then
    echo "  ok   $1"
  else
    echo "  FAIL $1 (expected '$3', got '$2')"
    fail=1
  fi
}

echo "checking node-a HTTP surface"
health="$(curl -s "http://127.0.0.1:$a_http/health")"
check "GET /health (open)" "${health//[[:space:]]/}" '{"status":"ok"}'

code="$(signed_get "/nope")"
check "GET /nope -> 404" "$code" "404"

code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "http://127.0.0.1:$a_http/health")"
check "POST /health -> 405" "$code" "405"

code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$a_http/status")"
check "GET /status unsigned -> 401" "$code" "401"

echo "checking node-b handshook node-a"
status="$(wait_for_signed_status || true)"
case "$status" in
  *'"peer_count":1'*) echo "  ok   node-a reports 1 peer" ;;
  *) echo "  FAIL node-a did not register the peer: $status"; fail=1 ;;
esac

if grep -q "handshake complete" "$logs/b.log"; then
  echo "  ok   node-b logged the outbound handshake"
else
  echo "  FAIL node-b did not log a handshake"
  fail=1
fi

echo "stopping nodes (logs in $logs)"
cleanup
pid_a=""
pid_b=""

if [ "$fail" -ne 0 ]; then
  echo "SMOKE TEST FAILED"
  exit 1
fi

echo "SMOKE TEST PASSED"
