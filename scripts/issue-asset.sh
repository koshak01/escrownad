#!/bin/zsh
# Issues a lot as a verified asset (Cleanverse A-Token) and follows the request
# until it is minted. The application does this by itself when an operator
# approves a listing; this script is for checking the path by hand.
#
# Usage: ./scripts/issue-asset.sh "<name>" "<SYMBOL>"
set -e

NAME="${1:-EscrowNad test lot}"
SYMBOL="${2:-ENTEST}"

psql_q() { psql -h 127.0.0.1 -U html -d escrownad.com -tAc "$1"; }
API_ID=$(psql_q "select cnt_value_json->>'api_id' from constants where cnt_code='cleanverse'")
API_KEY=$(psql_q "select cnt_value_json->>'api_key' from constants where cnt_code='cleanverse'")
ADMIN=$(psql_q "select cnt_value_json->>'treasury' from constants where cnt_code='chain'")
BASE="https://uatapi.cleanverse.com/api/cooperate"

KEY_HEX=$(printf '%s' "$API_KEY" | base64 -d | xxd -p -c 64)
PLAIN=$(cat <<JSON
{"chain":"monad","token_name":"$NAME","token_symbol":"$SYMBOL","decimals":6,"admin_address":"$ADMIN","icon":"https://escrownad.com/static/images/icon-512.png","rule":{"allowed_group":"","allowed_sub_group":"","min_tier":1,"min_sub_tier":0,"is_black_list":false,"countries":[]}}
JSON
)
CIPHER=$(printf '%s' "$PLAIN" | openssl enc -aes-256-cbc -K "$KEY_HEX" \
         -iv 00000000000000000000000000000000 -base64 -A)

echo "--- POST /atoken/launch ---"
RESP=$(curl -s -m 90 -X POST "$BASE/atoken/launch" \
  -H "Content-Type: application/json" -H "api-id: $API_ID" \
  -H "X-Request-ID: $(uuidgen)" -d "{\"data\":\"$CIPHER\"}")
echo "$RESP"

REQ=$(printf '%s' "$RESP" | python3 -c "import json,sys; print((json.load(sys.stdin).get('data') or {}).get('requestId',''))" 2>/dev/null)
[ -z "$REQ" ] && exit 1

echo "--- following request $REQ ---"
for i in 1 2 3 4 5 6 7 8 9 10; do
  sleep 6
  ST=$(curl -s -m 30 "$BASE/atoken/query_apply_status/$REQ" \
       -H "api-id: $API_ID" -H "X-Request-ID: $(uuidgen)")
  printf '%s\n' "$ST" | python3 -c "
import json,sys
d=json.load(sys.stdin); a=d.get('data') or {}
print(' ', a.get('applyStatus','?'), a.get('atokenAddress') or '')
" 2>/dev/null || echo "  (unreadable)"
  printf '%s' "$ST" | grep -q ISSUED && break
done
