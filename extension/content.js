// Argus content script — DOM layer. Snapshot of visible interactive elements
// with CSS selector paths. Password fields are never read or filled here.

(() => {
  if (window.__argusJsV === 3) return;
  window.__argusJsV = 3;

  window.__argusFind = (sel) => {
    try {
      const direct = document.querySelector(sel);
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
        hit = root.querySelector(sel);
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

  function selFor(el) {
    let path = "";

    for (let n = el; n && n !== document.body; n = n.parentElement) {
      if (n.id) {
        path = `#${CSS.escape(n.id)}${path ? " > " + path : ""}`;
        break;
      }

      const p = n.parentElement;

      if (!p) break;

      let s = n.tagName.toLowerCase();
      const sib = [...p.children].filter((c) => c.tagName === n.tagName);

      if (sib.length > 1) s += `:nth-of-type(${sib.indexOf(n) + 1})`;

      path = path ? `${s} > ${path}` : s;
    }

    return path;
  }

  window.__argusSnapshot = () => {
    const sel =
      'a, button, input, textarea, select, summary, [role="button"], [role="tab"], [role="search"], [role="combobox"], [role="switch"], [role="option"], [draggable="true"], [onclick], [aria-expanded], [contenteditable="true"]';
    const els = [...document.querySelectorAll(sel)];
    const out = [];

    for (const el of els) {
      const r = el.getBoundingClientRect();

      if (r.width === 0 || r.height === 0) continue;
      if (el.disabled) continue;

      const label = (
        el.innerText ||
        el.value ||
        el.placeholder ||
        el.getAttribute("aria-label") ||
        el.getAttribute("title") ||
        ""
      )
        .trim()
        .replace(/\s+/g, " ")
        .slice(0, 60);

      out.push({
        kind:
          el.tagName.toLowerCase() === "input" && el.type
            ? `input ${el.type}`
            : el.tagName.toLowerCase(),
        label,
        path: selFor(el),
      });
    }

    return out.slice(0, 100);
  };
})();
