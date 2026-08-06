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
    if (p.isPhantom) return "Phantom";
    if (p.isMetaMask) return "MetaMask";
    if (p.isRabby) return "Rabby";
    if (p.isCoinbaseWallet) return "Coinbase";
    return "wallet";
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
    if (/invalid formatting|cannot be shown/i.test(text)) {
      return "Wallet rejected the sign payload — try again or use MetaMask";
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

  /** Phantom EVM requires hex UTF-8 for personal_sign (plain string → invalid formatting). */
  function utf8ToHex(str) {
    const bytes = new TextEncoder().encode(str);
    let hex = "0x";
    for (let i = 0; i < bytes.length; i++) {
      hex += bytes[i].toString(16).padStart(2, "0");
    }
    return hex;
  }

  async function personalSign(provider, message, address) {
    // EIP-1193 / Phantom docs: hex-encoded UTF-8 string, then address.
    // Wallet displays decoded text; server ecrecover uses the same UTF-8 bytes.
    const msgHex = utf8ToHex(message);
    try {
      return await provider.request({
        method: "personal_sign",
        params: [msgHex, address],
      });
    } catch (e1) {
      // Some injectors still accept raw UTF-8 (MetaMask)
      const msg = (e1 && e1.message) || String(e1);
      if (/invalid formatting|invalid params|must provide/i.test(msg)) {
        return provider.request({
          method: "personal_sign",
          params: [message, address],
        });
      }
      throw e1;
    }
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
      const address = await requestAccounts(provider);
      rememberAddress(address);
      setStatus("Challenge…");
      const ch = await window.ws.request("wallet_challenge", { address });
      if (!ch || !ch.message) throw new Error("Empty challenge from server");
      setStatus("Sign in " + name + "…");
      const signature = await personalSign(provider, ch.message, address);
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
