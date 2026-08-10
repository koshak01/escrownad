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
      return "Phantom could not show the sign popup. Hard-refresh (Ctrl+Shift+R), enable EVM/Testnet Mode in Phantom, or connect with MetaMask.";
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

  /**
   * EIP-712 Login typed data — Phantom EVM handles this reliably.
   * personal_sign often dies with "invalid formatting" / 登录失败 on Phantom.
   * Must match server eip712_login_digest exactly.
   */
  function buildLoginTypedData(address, nonce, chainId) {
    return {
      types: {
        EIP712Domain: [
          { name: "name", type: "string" },
          { name: "version", type: "string" },
          { name: "chainId", type: "uint256" },
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
        chainId: Number(chainId) || 10143,
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
    // EIP-1193: [address, typedDataJson]
    return provider.request({
      method: "eth_signTypedData_v4",
      params: [address, payload],
    });
  }

  /**
   * personal_sign fallback — try common encodings if typed data is unavailable.
   * Server ecrecover uses the original UTF-8 nonce string (not hex).
   */
  async function personalSign(provider, message, address) {
    if (typeof message !== "string" || !message) {
      throw new Error("Empty sign-in message from server");
    }
    const checksum = address;
    const lower = String(address).toLowerCase();
    const msgHex = utf8ToHex(message);
    const phantom = isPhantomProvider(provider);

    const attempts = phantom
      ? [
          [msgHex, lower],
          [msgHex, checksum],
          [message, lower],
          [message, checksum],
        ]
      : [
          [message, checksum],
          [message, lower],
          [msgHex, lower],
          [msgHex, checksum],
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
   * Sign the login challenge: EIP-712 first (Phantom-friendly), then personal_sign.
   * Returns { signature, sign_kind, chain_id }.
   */
  async function signLogin(provider, address, challenge) {
    const nonce =
      (challenge && (challenge.nonce || challenge.message)) ||
      (typeof challenge === "string" ? challenge : "");
    if (!nonce) throw new Error("Empty sign-in nonce from server");
    const chainId = (challenge && challenge.chain_id) || 10143;

    // 1) Typed data — primary path for Phantom
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

    // 2) personal_sign fallback
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
