#!/bin/sh
# The edges of the dependency graph:
#   turnstile-wire stays pure: no bus, async runtime, caller identity, socket or device library.
#   turnstile-client is what palmrest and keycap link, so it must not drag in the bus, an async
#   runtime, the caller-identity crates or the daemon's libraries.
#   turnstile is a root service: it may read /proc through peephole, but must never link the bus or an
#   async runtime.
set -eu
cd "$(dirname "$0")/.."

# reaches CRATE PATTERN: the dependencies of CRATE (normal ones, build scripts excluded) whose name
# matches the extended regular expression PATTERN. A failing `cargo tree` stops the script instead of
# reading as "nothing found".
reaches() {
    tree=$(cargo tree -p "$1" -e normal --prefix none)
    echo "$tree" | grep -E "^($2) " || true
}

bad=$(reaches turnstile-wire 'zbus|tokio|async-std|smol|peephole[a-z-]*|rustix|libc|input-linux[a-z-]*|listenfd|nix')
[ -z "$bad" ] || { echo "check-boundary: turnstile-wire reaches: $bad"; exit 1; }

bad=$(reaches turnstile-client 'zbus|tokio|async-std|smol|peephole[a-z-]*|input-linux[a-z-]*|listenfd|nix')
[ -z "$bad" ] || { echo "check-boundary: turnstile-client reaches: $bad"; exit 1; }

bad=$(reaches turnstile 'zbus|tokio|async-std|smol|peephole-bus')
[ -z "$bad" ] || { echo "check-boundary: turnstile reaches: $bad"; exit 1; }

echo "check-boundary: ok"
