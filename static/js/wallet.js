/**
 * EscrowNad — MetaMask / EIP-1193 wallet login (Monad EVM).
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

  function hasProvider() {
    return !!(window.ethereum && window.ethereum.request);
  }

  async function requestAccounts() {
    if (!hasProvider()) {
      throw new Error("Установите MetaMask или другой EVM-кошелёк");
    }
    const accounts = await window.ethereum.request({ method: "eth_requestAccounts" });
    if (!accounts || !accounts.length) throw new Error("кошелёк не вернул адрес");
    return accounts[0];
  }

  async function personalSign(message, address) {
    // eth_sign / personal_sign: some wallets want [msg, addr], MetaMask [addr, msg] for personal_sign
    return window.ethereum.request({
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

  async function connect(redirectAfter) {
    try {
      setStatus("запрос кошелька…");
      await waitWs(8000);
      const address = await requestAccounts();
      setAddress(address);
      setStatus("челлендж…");
      const ch = await window.ws.request("wallet_challenge", { address });
      if (!ch || !ch.message) throw new Error("пустой челлендж");
      setStatus("подпись…");
      const signature = await personalSign(ch.message, address);
      setStatus("вход…");
      const resp = await window.ws.request("wallet_login", {
        address,
        signature,
        redirect_after: redirectAfter || "/cabinet/",
      });
      if (!resp || !resp.ok) throw new Error("вход отклонён");
      const msg = resp.is_new ? "кошелёк зарегистрирован" : "вход по кошельку";
      toast("success", msg);
      setStatus(msg);
      const dest = resp.redirect || redirectAfter || "/cabinet/";
      window.location.href = dest;
    } catch (e) {
      console.error("[wallet]", e);
      const text = (e && e.message) || String(e);
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
    connect(redirect);
  });

  window.EscrowWallet = { connect, hasProvider, shortAddr };
})();
