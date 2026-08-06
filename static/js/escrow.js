// escrow.js — the buyer funds the lock: approve + fund straight from the wallet.
//
// Money leaves the buyer's wallet for the EscrowLock contract. The server does
// not learn about it from the client's word: after sending we simply ask it to
// re-read the deal state on chain (ws deals_funded), and it sets the status only
// if the contract confirms the money is there.
//
// We never send the transaction hash to the server — it proves nothing.

import ws from 'forge/ws';
import ui from 'forge/ui-actions';
import toast from 'forge/toast';

// ERC-20 / EscrowLock function selectors (first 4 bytes of the keccak signature).
const SEL_APPROVE = '0x095ea7b3'; // approve(address,uint256)
const SEL_ALLOWANCE = '0xdd62ed3e'; // allowance(address,address)
const SEL_BALANCE = '0x70a08231'; // balanceOf(address)
// Selectors verified with `cast sig` — writing them from memory is not allowed.
const SEL_FUND = '0xfb998b39'; // fund(bytes32,address,uint256,uint64,bytes32)
const SEL_QUOTE = '0xed1bd76c'; // quote(uint256) → (total, fee)
const SEL_COMPLIANT = '0xa200e5d0'; // isCompliant(address) → bool

const pad = (hexNo0x) => hexNo0x.padStart(64, '0');
const addrArg = (addr) => pad(addr.toLowerCase().replace(/^0x/, ''));
const uintArg = (value) => pad(BigInt(value).toString(16));
const b32Arg = (hex) => pad(hex.replace(/^0x/, ''));

// We take the same provider as at sign-in (wallet.js), Phantom EVM first. No
// selection logic of our own — otherwise the payment would go from a different
// wallet than the one the person signed in with.
function provider() {
    const eth = (window.EscrowWallet && window.EscrowWallet.getProvider())
        || window.ethereum;
    if (!eth || !eth.request) {
        throw new Error('No wallet found. Open the page in Phantom or another EVM wallet.');
    }
    return eth;
}

async function currentAccount() {
    const accounts = await provider().request({ method: 'eth_requestAccounts' });
    if (!accounts || !accounts.length) throw new Error('Wallet is locked.');
    return accounts[0];
}

/** Switches the wallet to Monad, adding the network if needed. */
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
        // 4902 — the wallet does not know this network; try adding it
        if (e && (e.code === 4902 || e.code === -32603)) {
            try {
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
            } catch (addErr) {
                // Phantom refuses to add arbitrary networks: test networks are
                // enabled in its own settings, not on a page's request.
                throw new Error(
                    'Switch your wallet to Monad Testnet manually. '
                    + 'In Phantom: Settings → Developer Settings → Testnet Mode, then pick Monad Testnet.',
                );
            }
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

/** Waits for the transaction to land in a block and confirm successful execution. */
async function waitReceipt(txHash, { timeoutMs = 90000 } = {}) {
    const started = Date.now();
    for (;;) {
        const receipt = await provider().request({
            method: 'eth_getTransactionReceipt',
            params: [txHash],
        });
        if (receipt) {
            // status 0x0 — the transaction is in a block, but execution
            // reverted. Landing in a block does not count as success.
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

        // 0. verified identity. We ask the contract itself rather than our
        // server: the contract is the one that will refuse. With the gate off,
        // `isCompliant` returns true for anyone and this step changes nothing.
        btn.textContent = 'Checking identity…';
        const compliantHex = await ethCall(cfg.lock, SEL_COMPLIANT + addrArg(buyer));
        if (BigInt(compliantHex || '0x0') === 0n) {
            const url =
                (window.EscrowWallet && window.EscrowWallet.identityUrl()) ||
                'https://test-magiclink.cleanverse.com/';
            window.open(url, '_blank', 'noopener');
            throw new Error(
                'This wallet has no verified identity. Get one — the page opened in a new tab — then try again',
            );
        }

        // 1. ask the contract for the full amount: price plus the platform fee.
        // The rate is not duplicated on the client — one source of truth.
        const quoteHex = await ethCall(cfg.lock, SEL_QUOTE + uintArg(cfg.amount));
        const body = (quoteHex || '').replace(/^0x/, '');
        const total = BigInt('0x' + (body.slice(0, 64) || '0'));
        if (total < cfg.amount) throw new Error('Cannot read the total from the contract');

        // 2. is there enough USDC for the price together with the fees
        btn.textContent = 'Checking balance…';
        const balHex = await ethCall(cfg.usdc, SEL_BALANCE + addrArg(buyer));
        const balance = BigInt(balHex || '0x0');
        if (balance < total) {
            const need = Number(total) / 1e6;
            const have = Number(balance) / 1e6;
            throw new Error(`Not enough USDC: need ${need}, you have ${have}`);
        }

        // 3. allow the contract to pull the full amount
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

        // 4. money into the lock
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

        // 5. the server checks the fact against the contract itself
        btn.textContent = 'Confirming…';
        const resp = await ws.request('deals_funded', { hash: cfg.hash });
        ui.dispatch(resp);
    } catch (e) {
        const msg = e && (e.data?.message || e.message) || String(e);
        // 4001 — the user simply closed the wallet dialog; not an error
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
