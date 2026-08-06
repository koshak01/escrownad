#!/bin/zsh
# Registers EscrowLock as a compliance pool with the Cleanverse validator.
# Keys are read from the database and live only in this process's variables.
set -e
export PATH="$HOME/.foundry/bin:$PATH"

psql_q() { psql -h 127.0.0.1 -U html -d escrownad.com -tAc "$1"; }
API_ID=$(psql_q "select cnt_value_json->>'api_id' from constants where cnt_code='cleanverse'")
API_KEY=$(psql_q "select cnt_value_json->>'api_key' from constants where cnt_code='cleanverse'")
OWNER_KEY=$(psql_q "select cnt_value_json->>'observer_key' from constants where cnt_code='chain'")
LOCK=$(psql_q "select lower(cnt_value_json->>'lock') from constants where cnt_code='chain'")

BASE="https://uatapi.cleanverse.com/api/cooperate"
CHAIN="monad"

# 1. Owner's EIP-191 signature over lowercase(chain + contract_address)
MSG="${CHAIN}${LOCK}"
SIG=$(cast wallet sign --private-key "$OWNER_KEY" "$MSG")
echo "signed by owner: ${SIG:0:20}…"

# 2. Request body. The rule accepts any valid identity, with no group or
#    country restriction — this market is international.
PLAIN=$(cat <<JSON
{"chain":"$CHAIN","contract_address":"$LOCK","rule":{"allowed_group":"","allowed_sub_group":"","min_tier":1,"min_sub_tier":0,"is_black_list":false,"countries":[]},"owner_signature":"$SIG"}
JSON
)

# 3. AES-256-CBC, IV of sixteen zero bytes, key = base64-decoded api_key
KEY_HEX=$(printf '%s' "$API_KEY" | base64 -d | xxd -p -c 64)
CIPHER=$(printf '%s' "$PLAIN" | openssl enc -aes-256-cbc -K "$KEY_HEX" -iv 00000000000000000000000000000000 -base64 -A)

# 4. Send
REQ_ID=$(uuidgen)
echo "--- POST /validator/register ---"
curl -s -m 60 -X POST "$BASE/validator/register" \
  -H "Content-Type: application/json" \
  -H "api-id: $API_ID" \
  -H "X-Request-ID: $REQ_ID" \
  -d "{\"data\":\"$CIPHER\"}"
echo
echo "--- POST /validator/is_register ---"
curl -s -m 30 -X POST "$BASE/validator/is_register" \
  -H "Content-Type: application/json" \
  -H "api-id: $API_ID" \
  -H "X-Request-ID: $(uuidgen)" \
  -d "{\"chain\":\"$CHAIN\",\"contract_address\":\"$LOCK\"}"
echo
