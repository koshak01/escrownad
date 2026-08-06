#!/bin/zsh
# Deploys EscrowLock to Monad testnet. The key is read from the database and
# lives only in this process's variable — it never reaches the output.
set -e
export PATH="$HOME/.foundry/bin:$PATH"

psql_q() { psql -h 127.0.0.1 -U html -d escrownad.com -tAc "$1"; }
PSQL=psql_q
KEY=$(psql_q "select cnt_value_json->>'observer_key' from constants where cnt_code='chain'")
RPC=$(psql_q "select cnt_value_json->>'rpc' from constants where cnt_code='chain'")
USDC=$(psql_q "select cnt_value_json->>'usdc' from constants where cnt_code='chain'")
OBSERVER=$(psql_q "select cnt_value_json->>'observer' from constants where cnt_code='chain'")
TREASURY=$(psql_q "select cnt_value_json->>'treasury' from constants where cnt_code='chain'")
INSURANCE=$(psql_q "select cnt_value_json->>'insurance' from constants where cnt_code='chain'")

# the observer may be absent from the constants — then it is the key owner's address
if [ -z "$OBSERVER" ]; then
  OBSERVER=$(cast wallet address --private-key "$KEY")
fi

echo "usdc=$USDC observer=$OBSERVER treasury=$TREASURY insurance=$INSURANCE"
forge create contracts/EscrowLock.sol:EscrowLock \
  --rpc-url "$RPC" \
  --private-key "$KEY" \
  --broadcast \
  --constructor-args "$USDC" "$OBSERVER" "$TREASURY" "$INSURANCE"
