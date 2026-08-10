/**
 * EscrowNad — single EVM wallet connect (Monad).
 *
 * Auto-picks provider: Phantom EVM → MetaMask → any EIP-1193.
 * One button: data-wallet-connect. EN-only user strings.
 */
(function () {
  function shortAddr(a) {
    if (!a || a.length < 10) return a || "";
    return a.slice(0, 6) + "…" + a.slice(-4);
  }

  function setStatus(text) {
    document.querySelectorAll("[data-wallet-status]").forEach((el) => {
      el.textContent = text || "";
    });
  }

  function setAddress(addr) {
    document.querySelectorAll("[data-wallet-address]").forEach((el) => {
      el.textContent = shortAddr(addr);
      if (addr) el.dataset.full = addr;
    });
  }

  function toast(type, text) {
    if (window.DomEvents && window.DomEvents.toast) {
      window.DomEvents.toast({ type, text });
    }
  }

  /**
   * prefer: "phantom" | "metamask" | "" (auto)
   */
  function getProvider(prefer) {
    const want = (prefer || "").toLowerCase();

    const phantomEth =
      (window.phantom && window.phantom.ethereum) ||
      (window.ethereum && window.ethereum.isPhantom ? window.ethereum : null);

    const list =
      window.ethereum && Array.isArray(window.ethereum.providers)
        ? window.ethereum.providers.slice()
        : [];
    if (window.ethereum && !list.length && window.ethereum.request) {
      list.push(window.ethereum);
    }
    if (phantomEth && phantomEth.request && list.indexOf(phantomEth) === -1) {
      list.unshift(phantomEth);
    }

    function byFlag(flag) {
      return list.find((p) => p && p[flag] && typeof p.request === "function");
    }

    if (want === "phantom") {
      return (phantomEth && phantomEth.request && phantomEth) || byFlag("isPhantom") || null;
    }
    if (want === "metamask") {
      return (
        list.find((p) => p.isMetaMask && !p.isPhantom && p.request) ||
        byFlag("isMetaMask") ||
        null
      );
    }

    return (
      (phantomEth && phantomEth.request && phantomEth) ||
      byFlag("isPhantom") ||
      list.find((p) => p.isMetaMask && !p.isPhantom && p.request) ||
      list.find((p) => p && p.request) ||
      null
    );
  }

  function hasProvider(prefer) {
    return !!getProvider(prefer);
  }

  function providerName(p) {
    if (!p) return "";
    if (isPhantomProvider(p)) return "Phantom";
    if (p.isMetaMask) return "MetaMask";
    if (p.isRabby) return "Rabby";
    if (p.isCoinbaseWallet) return "Coinbase";
    return "wallet";
  }

  /** Phantom's ethereum provider sometimes lacks isPhantom — still is Phantom. */
  function isPhantomProvider(p) {
    if (!p) return false;
    if (p.isPhantom) return true;
    try {
      if (window.phantom && window.phantom.ethereum && window.phantom.ethereum === p) {
        return true;
      }
    } catch (_) {}
    return false;
  }

  function friendlyError(e) {
    const code = e && (e.code || (e.error && e.error.code));
    let text = (e && e.message) || String(e);
    if (code === 4001 || /user rejected|denied/i.test(text)) {
      return "Signature rejected";
    }
    if (/resource not available|not available|disconnected/i.test(text)) {
      return "Wallet not available — install Phantom or MetaMask and enable EVM";
    }
    if (/invalid formatting|cannot be shown|登录失败/i.test(text)) {
      return "Phantom refused the sign request. In Phantom: Settings → Developer → Testnet Mode ON, pick Monad Testnet, then hard-refresh this page (Ctrl+Shift+R) and try Connect Phantom again.";
    }
    if (/User rejected|user closed/i.test(text)) {
      return "Connection cancelled";
    }
    return text;
  }

  async function requestAccounts(provider) {
    const accounts = await provider.request({ method: "eth_requestAccounts" });
    if (!accounts || !accounts.length) throw new Error("No account returned by wallet");
    return accounts[0];
  }

  /** UTF-8 → 0x-hex for wallets that want hex personal_sign payloads. */
  function utf8ToHex(str) {
    const bytes = new TextEncoder().encode(str);
    let hex = "0x";
    for (let i = 0; i < bytes.length; i++) {
      hex += bytes[i].toString(16).padStart(2, "0");
    }
    return hex;
  }

  function errText(e) {
    return ((e && e.message) || (e && e.data && e.data.message) || String(e || "")).toLowerCase();
  }

  function isFormatError(e) {
    const t = errText(e);
    return /invalid formatting|invalid params|must provide|cannot be shown|登录失败|格式/.test(t);
  }

  const MONAD_TESTNET = {
    chainId: "0x279f", // 10143
    chainIdDec: 10143,
    chainName: "Monad Testnet",
    nativeCurrency: { name: "MON", symbol: "MON", decimals: 18 },
    rpcUrls: ["https://testnet-rpc.monad.xyz"],
    blockExplorerUrls: ["https://testnet.monadexplorer.com"],
  };

  /**
   * Switch Phantom/MetaMask onto Monad testnet before signing.
   * Signing while stuck on Solana-only or wrong EVM chain is a common
   * source of opaque "invalid formatting" / 登录失败 errors.
   */
  async function ensureMonadTestnet(provider) {
    const want = MONAD_TESTNET.chainId;
    try {
      const current = await provider.request({ method: "eth_chainId" });
      if (String(current).toLowerCase() === want) return;
    } catch (_) {
      /* continue and try switch */
    }
    try {
      await provider.request({
        method: "wallet_switchEthereumChain",
        params: [{ chainId: want }],
      });
      return;
    } catch (e) {
      const code = e && (e.code || (e.error && e.error.code));
      // 4902 — chain not added yet
      if (code === 4902 || code === -32603 || /unrecognized chain|not added/i.test(errText(e))) {
        await provider.request({
          method: "wallet_addEthereumChain",
          params: [
            {
              chainId: MONAD_TESTNET.chainId,
              chainName: MONAD_TESTNET.chainName,
              nativeCurrency: MONAD_TESTNET.nativeCurrency,
              rpcUrls: MONAD_TESTNET.rpcUrls,
              blockExplorerUrls: MONAD_TESTNET.blockExplorerUrls,
            },
          ],
        });
        return;
      }
      // User rejected switch — still try to sign; may fail later.
      console.warn("[wallet] could not switch to Monad testnet", e);
    }
  }

  /**
   * EIP-712 Login — matches server eip712_login_digest.
   * verifyingContract is the zero address (no on-chain verifier); included so
   * the domain shape matches what Phantom documents for typed data.
   */
  function buildLoginTypedData(address, nonce, chainId) {
    const cid = Number(chainId) || MONAD_TESTNET.chainIdDec;
    return {
      types: {
        EIP712Domain: [
          { name: "name", type: "string" },
          { name: "version", type: "string" },
          { name: "chainId", type: "uint256" },
          { name: "verifyingContract", type: "address" },
        ],
        Login: [
          { name: "wallet", type: "address" },
          { name: "nonce", type: "string" },
        ],
      },
      primaryType: "Login",
      domain: {
        name: "EscrowNad",
        version: "1",
        chainId: cid,
        verifyingContract: "0x0000000000000000000000000000000000000000",
      },
      message: {
        wallet: address,
        nonce: String(nonce),
      },
    };
  }

  async function signTypedLogin(provider, address, nonce, chainId) {
    const typed = buildLoginTypedData(address, nonce, chainId);
    const payload = JSON.stringify(typed);
    return provider.request({
      method: "eth_signTypedData_v4",
      params: [address, payload],
    });
  }

  /**
   * personal_sign — Phantom docs use THREE params: [hexMsg, from, password].
   * Two-param MetaMask style is what we had; Phantom then answers
   * "invalid formatting" / Chinese 登录失败.
   * Server ecrecover uses the original UTF-8 nonce (not the hex wrapper).
   */
  async function personalSign(provider, message, address) {
    if (typeof message !== "string" || !message) {
      throw new Error("Empty sign-in message from server");
    }
    const msgHex = utf8ToHex(message);
    const phantom = isPhantomProvider(provider);
    const from = address;

    // Phantom official example (docs):
    //   params: [msgHex, from, 'Example password']
    const attempts = phantom
      ? [
          [msgHex, from, "EscrowNad"],
          [msgHex, from, ""],
          [msgHex, from],
          [message, from, "EscrowNad"],
          [message, from],
        ]
      : [
          [message, from],
          [msgHex, from],
          [msgHex, from, ""],
        ];

    let lastErr = null;
    for (let i = 0; i < attempts.length; i++) {
      try {
        return await provider.request({
          method: "personal_sign",
          params: attempts[i],
        });
      } catch (e) {
        lastErr = e;
        const code = e && (e.code || (e.error && e.error.code));
        if (code === 4001 || /user rejected|denied|cancelled|canceled/i.test(errText(e))) {
          throw e;
        }
        if (isFormatError(e) || i < attempts.length - 1) {
          continue;
        }
        throw e;
      }
    }
    throw lastErr || new Error("personal_sign failed");
  }

  /**
   * Sign login: switch to Monad → EIP-712 → personal_sign (Phantom 3-arg).
   */
  async function signLogin(provider, address, challenge) {
    const nonce =
      (challenge && (challenge.nonce || challenge.message)) ||
      (typeof challenge === "string" ? challenge : "");
    if (!nonce) throw new Error("Empty sign-in nonce from server");
    const chainId = (challenge && challenge.chain_id) || MONAD_TESTNET.chainIdDec;

    await ensureMonadTestnet(provider);

    try {
      const signature = await signTypedLogin(provider, address, nonce, chainId);
      return { signature, sign_kind: "typed", chain_id: chainId, nonce };
    } catch (e1) {
      const code = e1 && (e1.code || (e1.error && e1.error.code));
      if (code === 4001 || /user rejected|denied|cancelled|canceled/i.test(errText(e1))) {
        throw e1;
      }
      console.warn("[wallet] eth_signTypedData_v4 failed, trying personal_sign", e1);
    }

    const signature = await personalSign(provider, nonce, address);
    return { signature, sign_kind: "personal", chain_id: chainId, nonce };
  }

  async function waitWs(timeoutMs) {
    const start = Date.now();
    while (!window.ws || typeof window.ws.request !== "function") {
      if (Date.now() - start > timeoutMs) throw new Error("WebSocket not ready");
      await new Promise((r) => setTimeout(r, 50));
    }
  }

  const STORAGE_KEY = "escrownad_wallet";
  const IDENTITY_KEY = "escrownad_identity";
  const IDENTITY_URL_KEY = "escrownad_identity_url";

  function rememberAddress(address) {
    try {
      if (address) sessionStorage.setItem(STORAGE_KEY, address);
      else sessionStorage.removeItem(STORAGE_KEY);
    } catch (_) {}
    setAddress(address || "");
  }

  /// Keeps the identity status for this tab, so pages rendered after the
  /// redirect can show the "get verified" prompt without asking again.
  function rememberIdentity(verified, url) {
    try {
      sessionStorage.setItem(IDENTITY_KEY, verified ? "1" : "0");
      if (url) sessionStorage.setItem(IDENTITY_URL_KEY, url);
      else sessionStorage.removeItem(IDENTITY_URL_KEY);
    } catch (_) {}
  }

  /// Identity status for this tab: `true`, `false`, or `null` when unknown.
  function identityStatus() {
    try {
      const raw = sessionStorage.getItem(IDENTITY_KEY);
      if (raw === null) return null;
      return raw === "1";
    } catch (_) {
      return null;
    }
  }

  /// Where to send someone who needs an identity.
  function identityUrl() {
    try {
      return sessionStorage.getItem(IDENTITY_URL_KEY) || "";
    } catch (_) {
      return "";
    }
  }

  function restoreAddress() {
    try {
      const a = sessionStorage.getItem(STORAGE_KEY);
      if (a) setAddress(a);
    } catch (_) {}
  }

  async function connect(redirectAfter, prefer) {
    try {
      setStatus("Requesting wallet…");
      await waitWs(8000);
      const provider = getProvider(prefer);
      if (!provider) {
        // Do not open external sites during connect — stay on EscrowNad.
        throw new Error("Install Phantom (EVM enabled) or another EVM wallet");
      }
      const name = providerName(provider);
      setStatus("Connecting " + name + "…");
      // Prefer Phantom's own ethereum object — window.ethereum may be a proxy
      // that signs badly or routes to the wrong chain.
      let signProvider = provider;
      if (
        isPhantomProvider(provider) &&
        window.phantom &&
        window.phantom.ethereum &&
        window.phantom.ethereum.request
      ) {
        signProvider = window.phantom.ethereum;
      }
      const address = await requestAccounts(signProvider);
      rememberAddress(address);
      setStatus("Challenge…");
      const ch = await window.ws.request("wallet_challenge", { address });
      const challenge =
        ch && typeof ch === "object"
          ? ch
          : typeof ch === "string"
            ? { message: ch, nonce: ch }
            : null;
      if (!challenge || !(challenge.nonce || challenge.message)) {
        console.error("[wallet] bad challenge payload", ch);
        throw new Error("Empty sign-in challenge from server");
      }
      setStatus("Sign in " + name + "…");
      console.info("[wallet] sign", {
        name,
        phantom: isPhantomProvider(signProvider),
        prefer: challenge.prefer,
        nonce: String(challenge.nonce || challenge.message).slice(0, 16),
      });
      // EIP-712 first (works on Phantom); personal_sign only as fallback.
      // Never jump to Cleanverse magiclink from here — that page has its own
      // broken-for-us wallet UI; CVI link stays on our market gate only.
      const signed = await signLogin(signProvider, address, challenge);
      setStatus("Signing in…");
      const destDefault = "/deals/";
      const resp = await window.ws.request("wallet_login", {
        address,
        signature: signed.signature,
        sign_kind: signed.sign_kind,
        chain_id: signed.chain_id,
        redirect_after: redirectAfter || destDefault,
      });
      if (!resp || !resp.ok) throw new Error("Sign-in rejected");
      const msg = resp.is_new
        ? "Wallet registered (" + name + ")"
        : "Signed in with " + name;
      toast("success", msg);
      setStatus(msg);
      if (resp.verified === false && resp.verify_url) {
        rememberIdentity(false, resp.verify_url);
        toast(
          "warning",
          "Signed in. A Cleanverse identity is still required to open the market — use Get verified on the next screen.",
        );
        // Stay on our site. Do NOT window.open magiclink (user stays in control;
        // that third-party page has its own wallet sign UI).
      } else if (resp.verified === true) {
        rememberIdentity(true, null);
      }
      window.location.href = resp.redirect || redirectAfter || destDefault;
    } catch (e) {
      console.error("[wallet]", e);
      const text = friendlyError(e);
      setStatus(text);
      toast("error", text);
    }
  }

  async function logout() {
    try {
      await waitWs(3000);
      if (window.ws && window.ws.request) {
        await window.ws.request("logout", {}).catch(() => {});
      }
    } finally {
      rememberAddress("");
      window.location.href = "/";
    }
  }

  document.addEventListener("click", (ev) => {
    const logoutEl =
      ev.target && ev.target.closest && ev.target.closest("[data-wallet-logout]");
    if (logoutEl) {
      ev.preventDefault();
      logout();
      return;
    }
    const el = ev.target && ev.target.closest && ev.target.closest("[data-wallet-connect]");
    if (!el) return;
    ev.preventDefault();
    const redirect =
      el.getAttribute("data-redirect-after") ||
      el.dataset.redirectAfter ||
      "/deals/";
    const prefer =
      el.getAttribute("data-wallet-prefer") || el.dataset.walletPrefer || "";
    connect(redirect, prefer);
  });

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", restoreAddress);
  } else {
    restoreAddress();
  }

  window.EscrowWallet = {
    connect,
    logout,
    hasProvider,
    getProvider,
    shortAddr,
    providerName,
    identityStatus,
    identityUrl,
  };
})();
