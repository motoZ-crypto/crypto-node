#!/usr/bin/env bash
#
# rotate-keys.sh — generate session keys + proof for a validator account.
#
# Usage:
#   scripts/rotate-keys.sh <SS58_ADDRESS> [RPC_URL]
#
# Example:
#   scripts/rotate-keys.sh 5CiPPseXPECbkjWCa6MnjNokrgYjMqmKndv2rSnekmSK2DjL
#   scripts/rotate-keys.sh 5Grw...   http://127.0.0.1:9944
#
# Output: the `keys` and `proof` hex strings to feed into
#         `session.setKeys(keys, proof)`.
#
# Notes:
#   * The SS58 address MUST be the same account that will sign the
#     `session.setKeys` extrinsic. The proof is a proof-of-possession
#     bound to that account; using a different signer => InvalidProof.
#   * The node must run with `--rpc-methods unsafe` (or `--unsafe-rpc-external`)
#     so that `author_rotateKeysWithOwner` is exposed.
#
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <SS58_ADDRESS> [RPC_URL]" >&2
  exit 2
fi

ADDR="$1"
RPC="${2:-http://127.0.0.1:9944}"

# --- Decode SS58 -> 32-byte raw public key (no external deps) ---------------
PUBKEY=$(python3 - "$ADDR" <<'PY'
import sys

ALPH = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"

def b58decode(s: str) -> bytes:
    n = 0
    for c in s.encode():
        n = n * 58 + ALPH.index(c)
    body = n.to_bytes((n.bit_length() + 7) // 8, "big")
    pad = len(s) - len(s.lstrip("1"))
    return b"\x00" * pad + body

addr = sys.argv[1]
raw = b58decode(addr)
# Substrate SS58: 1-byte prefix (for prefix < 64) + 32-byte pubkey + 2-byte checksum
if len(raw) != 35:
    sys.stderr.write(
        f"unexpected SS58 length {len(raw)} (only 1-byte prefix addresses are supported)\n"
    )
    sys.exit(1)
print("0x" + raw[1:33].hex())
PY
)

echo "owner pubkey: $PUBKEY" >&2

# --- Call author_rotateKeysWithOwner ----------------------------------------
RESP=$(curl -sS -H 'Content-Type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"author_rotateKeysWithOwner\",\"params\":[\"$PUBKEY\"]}" \
  "$RPC")

if echo "$RESP" | grep -q '"error"'; then
  echo "RPC error: $RESP" >&2
  exit 1
fi

KEYS=$(echo "$RESP"  | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['keys'])")
PROOF=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['proof'])")

cat <<EOF
keys:  $KEYS
proof: $PROOF

# Next step: in PolkadotJS Apps -> Developer -> Extrinsics, sign with $ADDR
#   session.setKeys(keys, proof)
EOF
