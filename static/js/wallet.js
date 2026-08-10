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
   * personal_sign — try every common encoding Phantom/MetaMask accept.
   *
   * Server ecrecover always uses the original UTF-8 challenge string (not hex).
   * Phantom is known to reject some shapes with "invalid formatting" while
   * accepting another; we do not guess — we try until one opens the popup.
   */
  async function personalSign(provider, message, address) {
    if (typeof message !== "string" || !message) {
      throw new Error("Empty sign-in message from server");
    }
    const checksum = address;
    const lower = String(address).toLowerCase();
    const msgHex = utf8ToHex(message);
    const phantom = isPhantomProvider(provider);

    // Each entry: [data, from]
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
        // User rejected — stop, do not spam popups.
        const code = e && (e.code || (e.error && e.error.code));
        if (code === 4001 || /user rejected|denied|cancelled|canceled/i.test(errText(e))) {
          throw e;
        }
        // Format/params errors: try next shape.
        if (isFormatError(e) || i < attempts.length - 1) {
          continue;
        }
        throw e;
      }
    }
    throw lastErr || new Error("personal_sign failed");
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
        window.open("https://phantom.app/", "_blank", "noopener");
        throw new Error("Install Phantom, MetaMask, or another EVM wallet");
      }
      const name = providerName(provider);
      setStatus("Connecting " + name + "…");
      // Prefer Phantom's own ethereum object when we think we have Phantom —
      // window.ethereum may be a multi-provider proxy that signs badly.
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
      // Challenge may be { message } or a plain string depending on envelope.
      const challengeMsg =
        typeof ch === "string"
          ? ch
          : ch && typeof ch.message === "string"
            ? ch.message
            : ch && ch.data && typeof ch.data.message === "string"
              ? ch.data.message
              : null;
      if (!challengeMsg) {
        console.error("[wallet] bad challenge payload", ch);
        throw new Error("Empty sign-in message from server");
      }
      setStatus("Sign in " + name + "…");
      console.info("[wallet] sign", {
        name,
        phantom: isPhantomProvider(signProvider),
        msgLen: challengeMsg.length,
        msgPreview: challengeMsg.slice(0, 24),
      });
      const signature = await personalSign(signProvider, challengeMsg, address);
      setStatus("Signing in…");
      const destDefault = "/deals/";
      const resp = await window.ws.request("wallet_login", {
        address,
        signature,
        redirect_after: redirectAfter || destDefault,
      });
      if (!resp || !resp.ok) throw new Error("Sign-in rejected");
      const msg = resp.is_new
        ? "Wallet registered (" + name + ")"
        : "Signed in with " + name;
      toast("success", msg);
      setStatus(msg);
      // Deals require a Cleanverse verified identity on both sides — the escrow
      // contract itself refuses to fund otherwise. Say so now, while the person
      // is still here, rather than letting them discover it on a failed
      // transaction. `verified === null` means we could not ask; stay quiet then.
      if (resp.verified === false && resp.verify_url) {
        rememberIdentity(false, resp.verify_url);
        toast(
          "warning",
          "This wallet has no verified identity yet — deals require one",
        );
        // Open Cleanverse self-service; the market page will still show the gate
        // until they reconnect with a valid CVI.
        window.open(resp.verify_url, "_blank", "noopener");
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
