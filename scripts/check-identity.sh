#!/bin/zsh
# Asks Cleanverse whether a wallet holds a verified identity, and asks our own
# contract whether it would let that wallet into a deal.
#
# Usage: ./scripts/check-identity.sh 0xWALLET [0xWALLET…]
# With no arguments, checks every wallet the project knows about.
set -e
export PATH="$HOME/.foundry/bin:$PATH"

psql_q() { psql -h 127.0.0.1 -U html -d escrownad.com -tAc "$1"; }
API_ID=$(psql_q "select cnt_value_json->>'api_id' from constants where cnt_code='cleanverse'")
RPC=$(psql_q "select cnt_value_json->>'rpc' from constants where cnt_code='chain'")
LOCK=$(psql_q "select cnt_value_json->>'lock' from constants where cnt_code='chain'")
BASE="https://uatapi.cleanverse.com/api/cooperate"

wallets=("$@")
if [ ${#wallets[@]} -eq 0 ]; then
  wallets=(${(f)"$(psql_q "select distinct u2w_address from users2wallets where u2w_address is not null
                           union select cnt_value_json->>'demo_seller' from constants where cnt_code='chain'
                           union select cnt_value_json->>'treasury' from constants where cnt_code='chain'")"})
fi

for w in $wallets; do
  [ -z "$w" ] && continue
  api=$(curl -s -m 20 -X POST "$BASE/query_apass" \
        -H "Content-Type: application/json" -H "api-id: $API_ID" \
        -H "X-Request-ID: $(uuidgen)" \
        -d "{\"chain\":\"monad\",\"address\":\"$w\"}")
  verdict=$(printf '%s' "$api" | python3 -c "
import json,sys,time
d=json.load(sys.stdin)
if d.get('code')!='0000':
    print('no identity')
else:
    a=d.get('data') or {}
    ok = a.get('status')==1 and (not a.get('expirationTime') or a.get('expirationTime')>time.time())
    print(('valid, tier ' + str(a.get('tier'))) if ok else 'present but not valid')
" 2>/dev/null || echo "unreadable reply")
  chain=$(cast call "$LOCK" "isCompliant(address)(bool)" "$w" --rpc-url "$RPC" 2>/dev/null || echo "?")
  printf '%s  registry: %-22s contract lets in: %s\n' "$w" "$verdict" "$chain"
done
