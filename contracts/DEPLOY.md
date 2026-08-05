# Deploy notes — EscrowLock (USDC)

Расчётный актив — **USDC (ERC-20, 6 знаков)**. Родной MON только на газ.

## Контракты

| Файл | Роль |
|---|---|
| `EscrowLock.sol` | замок: fund / release / refund USDC |
| `IERC20.sol` | минимальный интерфейс токена |
| `MockUSDC.sol` | не используется — на Monad есть настоящий USDC от Circle |

## Адреса

### Monad testnet (chain id 10143) — развёрнуто 2026-08-06

| Что | Адрес |
|---|---|
| USDC (Circle) | `0x534b2f3A21130d7a60830c2Df862319e593943A3` |
| **EscrowLock** | `0x3CB2C5EA954C7711EfF621A784CD096E4E580be5` |
| Наблюдатель (он же owner) | `0xC1B8d6B5CbB542e0c8Ae89AA5aBa43518a3282d0` |

Транзакция развёртывания:
`0x88f1da3f2f9c39acd74e47d8b78f09192d4d150ee15569d716db9840b4a9cd9f`

### Monad mainnet (chain id 143) — ещё не развёрнуто

| Что | Адрес |
|---|---|
| USDC (Circle) | `0x754704Bc059F8C67012fEd69BC8A327a5aafb603` |
| EscrowLock | — |

## Проверенный прогон (testnet, 2026-08-06)

Полный цикл денег отработал:

| Шаг | Транзакция | Результат |
|---|---|---|
| `approve(lock, 1 USDC)` | `0x8bb2c3ea…1a6d49` | success |
| `fund(dealId, seller, 1 USDC, deadline, cond)` | `0x9bb38735…a1be2d` | state = 1 (Funded) |
| `release(dealId, ripeKey)` | `0x82dbdeb3…c97a1` | state = 2 (Released), газ 149 760 |

Баланс продавца `0xf04f…6cdb`: было `0`, стало `1.000000 USDC`.

## Развёртывание

```bash
export PATH="$HOME/.foundry/bin:$PATH"
set -a; source .env; set +a

forge create contracts/EscrowLock.sol:EscrowLock \
  --rpc-url "$MONAD_RPC" \
  --private-key "$OBSERVER_PRIVATE_KEY" \
  --broadcast \
  --constructor-args "$USDC_ADDRESS" "$OBSERVER_ADDRESS"
```

**Внимание:** `--broadcast` обязан идти ДО `--constructor-args` — иначе
парсер съедает флаг как третий аргумент конструктора.

Сборка контрактов: `forge build` (это forge из Foundry, не наша кузница).
`foundry.toml` указывает `src = "contracts"`, потому что в `src/` лежит Rust.

## Поток денег покупателя

1. `usdc.approve(escrowLock, amount)`
2. `escrowLock.fund(dealId, seller, amount, deadline, conditionHash)`
3. Наблюдатель по факту RIPE: `release(dealId, ripeKey)` → USDC продавцу

Аварийный выход: если наблюдатель молчит и срок вышел, покупатель сам
зовёт `refundAfterDeadline(dealId)` и забирает деньги.

## Что уходит в цепь

Только отпечатки: `dealId` (хэш сделки) и `conditionHash` (хэш условия).
Ни сети, ни организаций, ни описания лота в цепи нет — это остаётся в базе.
Суммы и адреса на публичной цепи видны всегда, скрыть их без zk нельзя.

## Переменные окружения

| Переменная | Назначение |
|---|---|
| `CHAIN_MODE` | `live` — работать с цепью, иначе mock |
| `MONAD_RPC` | `https://testnet-rpc.monad.xyz` |
| `MONAD_CHAIN_ID` | `10143` |
| `USDC_ADDRESS` | адрес USDC из таблицы выше |
| `ESCROW_LOCK_ADDRESS` | адрес замка из таблицы выше |
| `OBSERVER_PRIVATE_KEY` | ключ наблюдателя — **только в окружении**, не в git |

Живой режим включается, лишь когда заданы и адрес замка, и ключ. Иначе
сервис остаётся в mock и пишет об этом предупреждение — чтобы неполная
конфигурация не роняла прод.
