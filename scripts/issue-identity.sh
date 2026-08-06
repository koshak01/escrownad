#!/bin/zsh
# Issues a sandbox verified identity (A-Pass) to a wallet through Cleanverse.
#
# This is the issuer path, available because our sandbox role allows it. Real
# users take the other one: they walk to the self-service page themselves and
# no personal data ever passes through us. This script exists so the demo
# wallets can transact and so the full cycle can be shown end to end.
#
# Usage: ./scripts/issue-identity.sh 0xWALLET [0xWALLET…]
set -e
export PATH="$HOME/.foundry/bin:$PATH"

if [ $# -eq 0 ]; then
  echo "usage: $0 0xWALLET [0xWALLET…]" >&2
  exit 2
fi

psql_q() { psql -h 127.0.0.1 -U html -d escrownad.com -tAc "$1"; }
API_ID=$(psql_q "select cnt_value_json->>'api_id' from constants where cnt_code='cleanverse'")
API_KEY=$(psql_q "select cnt_value_json->>'api_key' from constants where cnt_code='cleanverse'")
BASE="https://uatapi.cleanverse.com/api/cooperate"

# The key is base64; AES-256-CBC with an IV of sixteen zero bytes.
KEY_HEX=$(printf '%s' "$API_KEY" | base64 -d | xxd -p -c 64)

# Valid for a year from now. Passed in, not computed inside the payload, so the
# same command twice produces the same request except for this field.
EXPIRES=$(( $(date +%s) + 365*24*3600 ))

for w in "$@"; do
  # customerId: at least 12 characters, letters and digits only — so the raw
  # address without its 0x prefix fits exactly.
  CUSTOMER=$(printf '%s' "$w" | sed 's/^0[xX]//' | tr 'A-F' 'a-f')
  PLAIN="{\"customerId\":\"$CUSTOMER\",\"expirationTime\":$EXPIRES,\"wallet\":{\"address\":\"$w\",\"chain\":\"monad\"}}"
  CIPHER=$(printf '%s' "$PLAIN" | openssl enc -aes-256-cbc -K "$KEY_HEX" \
           -iv 00000000000000000000000000000000 -base64 -A)

  echo "--- $w"
  curl -s -m 90 -X POST "$BASE/generate_apass" \
    -H "Content-Type: application/json" \
    -H "api-id: $API_ID" \
    -H "X-Request-ID: $(uuidgen)" \
    -d "{\"data\":\"$CIPHER\"}"
  echo
done
