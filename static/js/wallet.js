/**
 * EscrowNad — EVM wallet login (Monad).
 *
 * Providers (EIP-1193 personal_sign):
 *  1. Phantom EVM  — window.phantom.ethereum / isPhantom
 *  2. MetaMask / Rabby / others — window.ethereum
 *
 * Flow:
 *  1. eth_requestAccounts
 *  2. ws wallet_challenge { address }
 *  3. personal_sign(message, address)
 *  4. ws wallet_login { address, signature, redirect_after }
 *  5. location = redirect
 *
 * Hooks:
 *  <button data-wallet-connect data-redirect-after="/cabinet/">Кошелёк</button>
 *  <button data-wallet-connect data-wallet-prefer="phantom">…</button>
 *  <span data-wallet-status></span>
 *  <span data-wallet-address></span>
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
   * Pick EIP-1193 provider.
   * prefer: "phantom" | "metamask" | "" (auto: Phantom → MetaMask → any)
   */
  function getProvider(prefer) {
    const want = (prefer || "").toLowerCase();

    // Explicit Phantom EVM inject
    const phantomEth =
      (window.phantom && window.phantom.ethereum) ||
      (window.ethereum && window.ethereum.isPhantom ? window.ethereum : null);

    // Multi-provider (Chrome: several extensions share window.ethereum.providers)
    const list =
      window.ethereum && Array.isArray(window.ethereum.providers)
        ? window.ethereum.providers.slice()
        : [];
    if (window.ethereum && !list.length && window.ethereum.request) {
      list.push(window.ethereum);
    }
    if (phantomEth && phantomEth.request && !list.includes(phantomEth)) {
      list.unshift(phantomEth);
    }

    function byFlag(flag) {
      return list.find((p) => p && p[flag] && typeof p.request === "function");
    }

    if (want === "phantom") {
      return (
        (phantomEth && phantomEth.request && phantomEth) ||
        byFlag("isPhantom") ||
        null
      );
    }
    if (want === "metamask") {
      // MetaMask sets isMetaMask; Phantom sometimes also — prefer non-Phantom
      return (
        list.find((p) => p.isMetaMask && !p.isPhantom && p.request) ||
        byFlag("isMetaMask") ||
        null
      );
    }

    // Auto: Phantom first (operator prefers), then MetaMask, then any
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

  function installHint(prefer) {
    if ((prefer || "").toLowerCase() === "phantom" || !window.ethereum) {
      return "Установите Phantom (https://phantom.app) или MetaMask";
    }
    return "Установите Phantom, MetaMask или другой EVM-кошелёк";
  }

  async function requestAccounts(provider) {
    const accounts = await provider.request({ method: "eth_requestAccounts" });
    if (!accounts || !accounts.length) throw new Error("кошелёк не вернул адрес");
    return accounts[0];
  }

  async function personalSign(provider, message, address) {
    return provider.request({
      method: "personal_sign",
      params: [message, address],
    });
  }

  async function waitWs(timeoutMs) {
    const start = Date.now();
    while (!window.ws || typeof window.ws.request !== "function") {
      if (Date.now() - start > timeoutMs) throw new Error("WebSocket не готов");
      await new Promise((r) => setTimeout(r, 50));
    }
  }

  async function connect(redirectAfter, prefer) {
    try {
      setStatus("запрос кошелька…");
      await waitWs(8000);
      const provider = getProvider(prefer);
      if (!provider) {
        const hint = installHint(prefer);
        // Soft: open Phantom download if clearly no wallet
        if ((prefer || "").toLowerCase() === "phantom" || !window.ethereum) {
          window.open("https://phantom.app/", "_blank", "noopener");
        }
        throw new Error(hint);
      }
      const name = providerName(provider);
      setStatus(name + "…");
      const address = await requestAccounts(provider);
      setAddress(address);
      setStatus("челлендж…");
      const ch = await window.ws.request("wallet_challenge", { address });
      if (!ch || !ch.message) throw new Error("пустой челлендж");
      setStatus("подпись в " + name + "…");
      const signature = await personalSign(provider, ch.message, address);
      setStatus("вход…");
      const resp = await window.ws.request("wallet_login", {
        address,
        signature,
        redirect_after: redirectAfter || "/cabinet/",
      });
      if (!resp || !resp.ok) throw new Error("вход отклонён");
      const msg = resp.is_new
        ? "кошелёк зарегистрирован (" + name + ")"
        : "вход через " + name;
      toast("success", msg);
      setStatus(msg);
      const dest = resp.redirect || redirectAfter || "/cabinet/";
      window.location.href = dest;
    } catch (e) {
      console.error("[wallet]", e);
      // User rejected in wallet UI
      const code = e && (e.code || (e.error && e.error.code));
      let text = (e && e.message) || String(e);
      if (code === 4001 || /user rejected|denied|отклон/i.test(text)) {
        text = "подпись отклонена";
      }
      setStatus(text);
      toast("error", text);
    }
  }

  // Delegation — works for login modal HTML injected after DOMContentLoaded.
  document.addEventListener("click", (ev) => {
    const el = ev.target && ev.target.closest && ev.target.closest("[data-wallet-connect]");
    if (!el) return;
    ev.preventDefault();
    const redirect =
      el.getAttribute("data-redirect-after") ||
      el.dataset.redirectAfter ||
      "/cabinet/";
    const prefer =
      el.getAttribute("data-wallet-prefer") || el.dataset.walletPrefer || "";
    connect(redirect, prefer);
  });

  window.EscrowWallet = { connect, hasProvider, getProvider, shortAddr, providerName };
})();
