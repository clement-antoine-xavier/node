#!/usr/bin/env bash
#
# Live smoke test: boots two nodes, verifies the gRPC handshake, the HTTP
# endpoints, routing and method filtering, then tears everything down.
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

wait_for_status() {
  local url="$1" needle="$2" body=""
  for _ in $(seq 1 50); do
    body="$(curl -s "$url")"
    case "$body" in *"$needle"*) echo "$body"; return 0;; esac
    sleep 0.1
  done
  echo "$body"
  return 1
}

cargo build -q -p node

echo "starting node-a (p2p $a_p2p, http $a_http)"
"$bin" --node-id node-a --p2p-listen "127.0.0.1:$a_p2p" --client-listen "127.0.0.1:$a_http" \
  >"$logs/a.log" 2>&1 &
pid_a=$!
wait_for "http://127.0.0.1:$a_http/health" || { echo "node-a never came up"; exit 1; }

echo "starting node-b (p2p $b_p2p, http $b_http) -> peer node-a"
"$bin" --node-id node-b --p2p-listen "127.0.0.1:$b_p2p" --client-listen "127.0.0.1:$b_http" \
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
check "GET /health" "${health//[[:space:]]/}" '{"status":"ok"}'

code="$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$a_http/nope")"
check "GET /nope -> 404" "$code" "404"

code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "http://127.0.0.1:$a_http/health")"
check "POST /health -> 405" "$code" "405"

echo "checking node-b handshook node-a"
status="$(wait_for_status "http://127.0.0.1:$a_http/status" '"peer_count":1' || true)"
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
