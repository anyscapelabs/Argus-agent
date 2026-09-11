// Argus native-messaging bridge. The host sends req frames, this SW executes
// them via extension APIs and sends resp frames back. Reconnects on drop.

const HOST_NAME = "com.argus.browser";
const PING_MS = 20_000;
const NAV_TIMEOUT_MS = 20_000;
const SETTLE_MS = 2_500;

let port = null;
let pingTimer = null;

function send(msg) {
  if (!port) return;

  try {
    port.postMessage(msg);
  } catch {
    port = null;
    connect();
  }
}

function reply(id, ok, data, err) {
  const msg = { kind: "resp", id, ok };

  if (ok) {
    msg.data = data ?? null;
  }

  if (!ok) {
    msg.err = err ?? "unknown error";
  }

  send(msg);
}

function connect() {
  if (port) return;

  try {
    port = chrome.runtime.connectNative(HOST_NAME);
  } catch {
    setTimeout(connect, 5_000);
    return;
  }

  port.onMessage.addListener(onMsg);

  port.onDisconnect.addListener(() => {
    port = null;
    clearInterval(pingTimer);
    setTimeout(connect, 2_000);
  });

  pingTimer = setInterval(() => send({ kind: "ping" }), PING_MS);
}

function onMsg(msg) {
  if (!msg || msg.kind !== "req") return;

  handle(msg).then(
    (data) => reply(msg.id, true, data),
    (err) => reply(msg.id, false, null, String(err?.message ?? err))
  );
}

async function handle(msg) {
  const d = msg.data ?? {};

  switch (msg.method) {
    case "navigate":
      return nav(d.url, d.tabId);
    case "read":
      return readTab(d.tabId);
    case "snapshot":
      return snapTab(d.tabId);
    case "click":
      return clickEl(d.tabId, d.path);
    case "fill":
      return fillEl(d.tabId, d.path, d.text, !!d.submit);
    case "closeTab":
      await chrome.tabs.remove(d.tabId).catch(() => {});
      return {};
    default:
      throw new Error(`unknown method ${msg.method}`);
  }
}

async function hasTab(tabId) {
  return chrome.tabs
    .get(tabId)
    .then(() => true)
    .catch(() => false);
}

function waitLoad(tabId, timeoutMs) {
  return new Promise((resolve) => {
    const done = () => {
      chrome.tabs.onUpdated.removeListener(listener);
      resolve();
    };

    const listener = (id, info) => {
      if (id === tabId && info.status === "complete") done();
    };

    chrome.tabs.onUpdated.addListener(listener);
    setTimeout(done, timeoutMs);
  });
}

async function inject(tabId) {
  try {
    await chrome.scripting.executeScript({
      target: { tabId },
      files: ["content.js"],
    });
  } catch {
    // chrome:// and other restricted pages can't host content scripts
  }
}

async function pageTxt(tabId) {
  const [res] = await chrome.scripting.executeScript({
    target: { tabId },
    func: () => (document.body ? document.body.innerText : ""),
  });

  return res?.result ?? "";
}

async function nav(url, tabId) {
  let tab;

  if (tabId && (await hasTab(tabId))) {
    await chrome.tabs.update(tabId, { url });
    await waitLoad(tabId, NAV_TIMEOUT_MS);
    tab = await chrome.tabs.get(tabId);
  }

  if (!tab) {
    tab = await chrome.tabs.create({ url, active: true });
    await waitLoad(tab.id, NAV_TIMEOUT_MS);
    tab = await chrome.tabs.get(tab.id);
  }

  await inject(tab.id);
  await new Promise((r) => setTimeout(r, SETTLE_MS));

  const text = await pageTxt(tab.id).catch(() => "");

  return { tabId: tab.id, url: tab.url ?? url, title: tab.title ?? "", text };
}

async function readTab(tabId) {
  if (!(await hasTab(tabId))) {
    throw new Error("tab was closed — run browser.open again");
  }

  await inject(tabId);

  const tab = await chrome.tabs.get(tabId);
  const text = await pageTxt(tabId).catch(() => "");

  return { url: tab.url ?? "", title: tab.title ?? "", text };
}

async function snapTab(tabId) {
  if (!(await hasTab(tabId))) {
    throw new Error("tab was closed — run browser.open again");
  }

  await inject(tabId);
  await new Promise((r) => setTimeout(r, 200));

  const [res] = await chrome.scripting.executeScript({
    target: { tabId },
    func: () => window.__argusSnapshot(),
  });

  return res?.result ?? [];
}

async function runTab(tabId, func, args) {
  const [res] = await chrome.scripting.executeScript({
    target: { tabId },
    func,
    args,
  });

  return res?.result;
}

async function clickEl(tabId, path) {
  if (!(await hasTab(tabId))) {
    throw new Error("tab was closed — run browser.open again");
  }

  const out = await runTab(
    tabId,
    (sel) => {
      const el = document.querySelector(sel);
      if (!el) return "missing";

      el.scrollIntoView({ block: "center" });
      el.click();

      return "ok";
    },
    [path]
  );

  if (out === "missing") {
    throw new Error("stale ref — run browser.read for a fresh element list");
  }

  await waitLoad(tabId, SETTLE_MS + 3_000);
  await inject(tabId);

  return {};
}

function chkPwd(text) {
  const t = (text ?? "").toLowerCase();

  if (t.includes("password")) return "passwords are never typed by the agent";

  return null;
}

async function fillEl(tabId, path, text, submit) {
  if (!(await hasTab(tabId))) {
    throw new Error("tab was closed — run browser.open again");
  }

  if (chkPwd(text)) throw new Error(chkPwd(text));

  const out = await runTab(
    tabId,
    (sel, val, doSubmit) => {
      const el = document.querySelector(sel);
      if (!el) return "missing";

      el.scrollIntoView({ block: "center" });
      el.focus();

      // native setter so React-style value listeners fire
      const proto =
        el instanceof HTMLTextAreaElement
          ? HTMLTextAreaElement.prototype
          : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
      setter?.call(el, val);
      el.dispatchEvent(new Event("input", { bubbles: true }));
      el.dispatchEvent(new Event("change", { bubbles: true }));

      if (doSubmit) {
        const form = el.form;

        if (form && typeof form.requestSubmit === "function") {
          form.requestSubmit();
        }

        if (!form) {
          el.dispatchEvent(
            new KeyboardEvent("keydown", { key: "Enter", bubbles: true })
          );
          el.dispatchEvent(
            new KeyboardEvent("keyup", { key: "Enter", bubbles: true })
          );
        }
      }

      return "ok";
    },
    [path, text, submit]
  );

  if (out === "missing") {
    throw new Error("stale ref — run browser.read for a fresh element list");
  }

  if (submit) {
    await waitLoad(tabId, SETTLE_MS + 5_000);
    await inject(tabId);
  }

  if (!submit) {
    await new Promise((r) => setTimeout(r, 300));
  }

  return {};
}

connect();
