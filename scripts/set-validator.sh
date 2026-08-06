#!/bin/zsh
# Points EscrowLock at the Cleanverse CCP validator, switching the identity
# gate on. The key is read from the database and never reaches the output.
#
# Pass an address to set it, or `off` to disable the gate.
set -e
export PATH="$HOME/.foundry/bin:$PATH"

VALIDATOR="${1:-0xaC7e5179C2C7f03f209136886c172eb34F161792}"
if [ "$VALIDATOR" = "off" ]; then
  VALIDATOR=0x0000000000000000000000000000000000000000
fi

psql_q() { psql -h 127.0.0.1 -U html -d escrownad.com -tAc "$1"; }
KEY=$(psql_q "select cnt_value_json->>'observer_key' from constants where cnt_code='chain'")
RPC=$(psql_q "select cnt_value_json->>'rpc' from constants where cnt_code='chain'")
LOCK=$(psql_q "select cnt_value_json->>'lock' from constants where cnt_code='chain'")

echo "lock=$LOCK  validator=$VALIDATOR"
cast send "$LOCK" "setValidator(address)" "$VALIDATOR" \
  --rpc-url "$RPC" --private-key "$KEY" 2>&1 | grep -E "^(status|transactionHash|gasUsed)"

echo "--- as the contract now sees it ---"
echo -n "validator():          "; cast call "$LOCK" "validator()(address)" --rpc-url "$RPC"
echo -n "isCompliant(random):  "; cast call "$LOCK" "isCompliant(address)(bool)" \
  0x0000000000000000000000000000000000000001 --rpc-url "$RPC"
