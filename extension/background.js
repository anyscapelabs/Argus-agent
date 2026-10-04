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
    case "scroll":
      return scrollTab(d.tabId, d.dy);
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

async function scrollTab(tabId, dy) {
  if (!(await hasTab(tabId))) {
    throw new Error("tab was closed — run browser.open again");
  }

  await runTab(
    tabId,
    (amount) => {
      window.scrollBy(0, amount);
      return "ok";
    },
    [Number(dy) || 800]
  );

  await new Promise((r) => setTimeout(r, 400));

  return {};
}

async function clickEl(tabId, path) {
  if (!(await hasTab(tabId))) {
    throw new Error("tab was closed — run browser.open again");
  }

  const out = await runTab(
    tabId,
    (sel) => {
      const queryDeep = (s) => {
        try {
          const direct = document.querySelector(s);
          if (direct) return direct;
        } catch {
          return null;
        }
        const seen = new Set();
        const stack = [document];
        while (stack.length > 0) {
          const root = stack.pop();
          if (!root || seen.has(root)) continue;
          seen.add(root);
          let hit = null;
          try {
            hit = root.querySelector(s);
          } catch {
            hit = null;
          }
          if (hit) return hit;
          let els = [];
          try {
            els = [...root.querySelectorAll("*")];
          } catch {
            els = [];
          }
          for (const el of els) {
            if (el.shadowRoot) stack.push(el.shadowRoot);
            if (el.tagName === "IFRAME") {
              try {
                if (el.contentDocument) stack.push(el.contentDocument);
              } catch {
              }
            }
          }
        }
        return null;
      };

      const el = queryDeep(sel);
      if (!el) return "missing";

      el.scrollIntoView({ block: "center" });
      el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
      el.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, cancelable: true }));
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

  let out;
  try {
    out = await runTab(
    tabId,
    async (sel, val, doSubmit) => {
      const queryDeep = (s) => {
        try {
          const direct = document.querySelector(s);
          if (direct) return direct;
        } catch {
          return null;
        }
        const seen = new Set();
        const stack = [document];
        while (stack.length > 0) {
          const root = stack.pop();
          if (!root || seen.has(root)) continue;
          seen.add(root);
          let hit = null;
          try {
            hit = root.querySelector(s);
          } catch {
            hit = null;
          }
          if (hit) return hit;
          let els = [];
          try {
            els = [...root.querySelectorAll("*")];
          } catch {
            els = [];
          }
          for (const el of els) {
            if (el.shadowRoot) stack.push(el.shadowRoot);
            if (el.tagName === "IFRAME") {
              try {
                if (el.contentDocument) stack.push(el.contentDocument);
              } catch {
              }
            }
          }
        }
        return null;
      };

      const pressEnter = (el) => {
        for (const type of ["keydown", "keypress", "keyup"]) {
          const ev = new KeyboardEvent(type, {
            key: "Enter",
            code: "Enter",
            bubbles: true,
            cancelable: true,
          });
          try {
            Object.defineProperty(ev, "keyCode", { value: 13 });
            Object.defineProperty(ev, "which", { value: 13 });
          } catch {
          }
          el.dispatchEvent(ev);
        }
      };

      const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

      const readField = (node, isEditable) => {
        if (isEditable) return node.innerText ?? node.textContent ?? "";
        if ("value" in node) return node.value ?? "";
        return node.innerText ?? node.textContent ?? "";
      };

      const submitTail = async (node, isEditable, filled) => {
        pressEnter(node);
        await sleep(1200);
        if (readField(node, isEditable) !== filled) {
          return JSON.stringify({ status: "ok", actual: filled, submitted: "keys" });
        }
        const form = node.form;
        if (form && typeof form.requestSubmit === "function") {
          form.requestSubmit();
          return JSON.stringify({ status: "ok", actual: filled, submitted: "form" });
        }
        return JSON.stringify({ status: "submit-failed", actual: readField(node, isEditable) });
      };

      const el = queryDeep(sel);
      if (!el) return JSON.stringify({ status: "missing" });

      el.scrollIntoView({ block: "center" });

      const tag = (el.tagName || "").toUpperCase();
      const type = (el.type || "").toLowerCase();
      const editable =
        el.isContentEditable || el.getAttribute("contenteditable") === "true";

      if (tag === "INPUT" && (type === "checkbox" || type === "radio")) {
        el.click();
        return JSON.stringify({ status: "ok", actual: val });
      }

      if (tag === "SELECT") {
        const opts = [...el.options];
        const match = opts.find(
          (o) =>
            o.value === val || (o.text || "").trim() === (val || "").trim()
        );
        if (!match) return JSON.stringify({ status: "no-option" });
        el.focus();
        el.value = match.value;
        el.dispatchEvent(new Event("input", { bubbles: true }));
        el.dispatchEvent(new Event("change", { bubbles: true }));
        if (doSubmit) return await submitTail(el, false, val);
        return JSON.stringify({ status: "ok", actual: val });
      }

      if (editable) {
        el.focus();
        try {
          const range = document.createRange();
          range.selectNodeContents(el);
          const sel = window.getSelection();
          sel.removeAllRanges();
          sel.addRange(range);
        } catch {
        }
        let inserted = false;
        try {
          inserted = document.execCommand("insertText", false, val);
        } catch {
          inserted = false;
        }
        if (!inserted) {
          try {
            el.textContent = val;
            el.dispatchEvent(
              new InputEvent("input", {
                bubbles: true,
                cancelable: true,
                inputType: "insertText",
                data: val,
              })
            );
          } catch {
            el.textContent = val;
          }
        }
        const actual = (el.innerText ?? el.textContent ?? "").trim();
        if (doSubmit) return await submitTail(el, true, actual);
        return JSON.stringify({ status: "ok", actual });
      }

      if (
        tag !== "INPUT" &&
        tag !== "TEXTAREA" &&
        !(el instanceof HTMLInputElement) &&
        !(el instanceof HTMLTextAreaElement)
      ) {
        return JSON.stringify({ status: "unsupported" });
      }

      el.focus();
      try {
        el.click();
      } catch {
      }
      try {
        if (typeof el.select === "function") el.select();
      } catch {
      }

      const proto =
        el instanceof HTMLTextAreaElement
          ? HTMLTextAreaElement.prototype
          : HTMLInputElement.prototype;
      try {
        const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
        if (setter) setter.call(el, val);
        else el.value = val;
      } catch {
        el.value = val;
      }
      try {
        const tracker = el._valueTracker;
        if (tracker && typeof tracker.setValue === "function") {
          tracker.setValue("");
        }
      } catch {
      }
      try {
        el.dispatchEvent(
          new InputEvent("input", {
            bubbles: true,
            cancelable: true,
            inputType: "insertText",
            data: val,
          })
        );
      } catch {
        el.dispatchEvent(new Event("input", { bubbles: true }));
      }
      el.dispatchEvent(new Event("change", { bubbles: true }));
      if (!doSubmit) {
        return JSON.stringify({ status: "ok", actual: el.value ?? "" });
      }
      return await submitTail(el, false, el.value ?? "");
    },
    [path, text, submit]
    );
  } catch (e) {
    if (!submit) throw e;
    out = undefined;
  }

  let parsed = null;
  try {
    parsed = JSON.parse(out ?? "");
  } catch {
    parsed = null;
  }
  if (!parsed) {
    if (submit && out === undefined) {
      await waitLoad(tabId, SETTLE_MS + 5_000);
      await inject(tabId);
      return {};
    }
    throw new Error("stale ref — run browser.read for a fresh element list");
  }
  if (parsed.status === "missing") {
    throw new Error("stale ref — run browser.read for a fresh element list");
  }
  if (parsed.status === "unsupported") {
    throw new Error("that element takes no text — click it or pick a field");
  }
  if (parsed.status === "no-option") {
    throw new Error("no dropdown option matches that text");
  }
  if (parsed.status === "submit-failed") {
    throw new Error(
      "the text is in the field but sending didn't trigger — click the Send button element instead (find its ref with browser.read)"
    );
  }

  const actual = String(parsed.actual ?? "");
  const want = String(text ?? "");
  const landed =
    actual.includes(want) || (actual.length > 0 && want.startsWith(actual));
  if (!landed) {
    const got = actual.length > 120 ? `${actual.slice(0, 120)}…` : actual;
    throw new Error(
      `typing did not land (field shows ${got ? `"${got}"` : "empty"}) — the site may need one choice picked first, or the field is read-only`
    );
  }

  if (submit && parsed.submitted === "form") {
    await waitLoad(tabId, SETTLE_MS + 5_000);
    await inject(tabId);
  }

  if (!submit) {
    await new Promise((r) => setTimeout(r, 300));
  }

  return {};
}

connect();
