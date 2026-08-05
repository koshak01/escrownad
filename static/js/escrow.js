// escrow.js — оплата замка покупателем: approve + fund прямо из кошелька.
//
// Деньги уходят с кошелька покупателя на контракт EscrowLock. Сервер узнаёт
// об этом не со слов клиента: после отправки мы просто просим его перечитать
// состояние сделки в цепи (ws deals_funded), и он ставит статус только если
// контракт подтвердил, что деньги на месте.
//
// Хэш транзакции серверу не отправляем — он ничего не доказывает.

import ws from 'forge/ws';
import ui from 'forge/ui-actions';
import toast from 'forge/toast';

// Селекторы функций ERC-20 / EscrowLock (первые 4 байта keccak сигнатуры).
const SEL_APPROVE = '0x095ea7b3'; // approve(address,uint256)
const SEL_ALLOWANCE = '0xdd62ed3e'; // allowance(address,address)
const SEL_BALANCE = '0x70a08231'; // balanceOf(address)
// Селекторы сверены через `cast sig` — на глаз их писать нельзя.
const SEL_FUND = '0xfb998b39'; // fund(bytes32,address,uint256,uint64,bytes32)
const SEL_QUOTE = '0xed1bd76c'; // quote(uint256) → (total, fee)

const pad = (hexNo0x) => hexNo0x.padStart(64, '0');
const addrArg = (addr) => pad(addr.toLowerCase().replace(/^0x/, ''));
const uintArg = (value) => pad(BigInt(value).toString(16));
const b32Arg = (hex) => pad(hex.replace(/^0x/, ''));

function provider() {
    const eth = window.ethereum;
    if (!eth) throw new Error('No wallet found. Install MetaMask or Phantom.');
    return eth;
}

async function currentAccount() {
    const accounts = await provider().request({ method: 'eth_requestAccounts' });
    if (!accounts || !accounts.length) throw new Error('Wallet is locked.');
    return accounts[0];
}

/** Переключает кошелёк на Monad, при необходимости добавляя сеть. */
async function ensureNetwork(cfg) {
    const eth = provider();
    const current = await eth.request({ method: 'eth_chainId' });
    if (current.toLowerCase() === cfg.chainIdHex.toLowerCase()) return;
    try {
        await eth.request({
            method: 'wallet_switchEthereumChain',
            params: [{ chainId: cfg.chainIdHex }],
        });
    } catch (e) {
        // 4902 — сеть кошельку неизвестна, добавляем
        if (e && (e.code === 4902 || e.code === -32603)) {
            await eth.request({
                method: 'wallet_addEthereumChain',
                params: [{
                    chainId: cfg.chainIdHex,
                    chainName: 'Monad Testnet',
                    nativeCurrency: { name: 'MON', symbol: 'MON', decimals: 18 },
                    rpcUrls: [cfg.rpcUrl],
                    blockExplorerUrls: ['https://testnet.monadexplorer.com'],
                }],
            });
        } else {
            throw e;
        }
    }
}

async function ethCall(to, data) {
    return provider().request({ method: 'eth_call', params: [{ to, data }, 'latest'] });
}

async function sendTx(from, to, data) {
    return provider().request({
        method: 'eth_sendTransaction',
        params: [{ from, to, data }],
    });
}

/** Ждёт, пока транзакция попадёт в блок и подтвердит успешное исполнение. */
async function waitReceipt(txHash, { timeoutMs = 90000 } = {}) {
    const started = Date.now();
    for (;;) {
        const receipt = await provider().request({
            method: 'eth_getTransactionReceipt',
            params: [txHash],
        });
        if (receipt) {
            // status 0x0 — транзакция в блоке, но исполнение откатилось.
            // Попадание в блок успехом не считаем.
            if (receipt.status !== '0x1') {
                throw new Error(`Transaction ${txHash.slice(0, 10)}… reverted on execution`);
            }
            return receipt;
        }
        if (Date.now() - started > timeoutMs) {
            throw new Error('Timed out waiting for the transaction');
        }
        await new Promise((r) => setTimeout(r, 900));
    }
}

function readConfig(btn) {
    return {
        chainId: Number(btn.dataset.chainId),
        chainIdHex: btn.dataset.chainIdHex,
        rpcUrl: btn.dataset.rpcUrl,
        usdc: btn.dataset.usdc,
        lock: btn.dataset.lock,
        dealId: btn.dataset.dealId,
        conditionHash: btn.dataset.conditionHash,
        seller: btn.dataset.seller,
        amount: BigInt(btn.dataset.amountUnits),
        deadline: BigInt(btn.dataset.deadline),
        hash: btn.dataset.hash,
    };
}

async function payLock(btn) {
    const cfg = readConfig(btn);
    btn.disabled = true;
    const restore = btn.textContent;

    try {
        await ensureNetwork(cfg);
        const buyer = await currentAccount();

        // 1. полную сумму спрашиваем у контракта: цена + комиссия площадки.
        // Ставку не дублируем на клиенте — источник истины один, контракт.
        const quoteHex = await ethCall(cfg.lock, SEL_QUOTE + uintArg(cfg.amount));
        const body = (quoteHex || '').replace(/^0x/, '');
        const total = BigInt('0x' + (body.slice(0, 64) || '0'));
        if (total < cfg.amount) throw new Error('Cannot read the total from the contract');

        // 2. хватает ли USDC на цену вместе с комиссией
        btn.textContent = 'Checking balance…';
        const balHex = await ethCall(cfg.usdc, SEL_BALANCE + addrArg(buyer));
        const balance = BigInt(balHex || '0x0');
        if (balance < total) {
            const need = Number(total) / 1e6;
            const have = Number(balance) / 1e6;
            throw new Error(`Not enough USDC: need ${need}, you have ${have}`);
        }

        // 3. разрешение контракту списать полную сумму
        const allowHex = await ethCall(
            cfg.usdc,
            SEL_ALLOWANCE + addrArg(buyer) + addrArg(cfg.lock),
        );
        if (BigInt(allowHex || '0x0') < total) {
            btn.textContent = 'Approve in wallet…';
            const approveTx = await sendTx(
                buyer,
                cfg.usdc,
                SEL_APPROVE + addrArg(cfg.lock) + uintArg(total),
            );
            btn.textContent = 'Waiting for approve…';
            await waitReceipt(approveTx);
        }

        // 4. деньги в замок
        btn.textContent = 'Confirm deposit…';
        const fundData = SEL_FUND
            + b32Arg(cfg.dealId)
            + addrArg(cfg.seller)
            + uintArg(cfg.amount)
            + uintArg(cfg.deadline)
            + b32Arg(cfg.conditionHash);
        const fundTx = await sendTx(buyer, cfg.lock, fundData);
        btn.textContent = 'Locking USDC…';
        await waitReceipt(fundTx);

        // 5. сервер сам сверяет факт с контрактом
        btn.textContent = 'Confirming…';
        const resp = await ws.request('deals_funded', { hash: cfg.hash });
        ui.dispatch(resp);
    } catch (e) {
        const msg = e && (e.data?.message || e.message) || String(e);
        // 4001 — пользователь просто закрыл окно кошелька, это не ошибка
        if (e && e.code === 4001) {
            toast({ type: 'info', text: 'Cancelled' });
        } else {
            console.error('[escrow]', e);
            toast({ type: 'error', text: msg });
        }
        btn.textContent = restore;
        btn.disabled = false;
    }
}

function initAll() {
    document.querySelectorAll('[data-escrow-pay]').forEach((btn) => {
        if (btn.dataset.escrowBound) return;
        btn.dataset.escrowBound = '1';
        btn.addEventListener('click', () => payLock(btn));
    });
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initAll);
} else {
    initAll();
}
document.addEventListener('html-replaced', initAll);
