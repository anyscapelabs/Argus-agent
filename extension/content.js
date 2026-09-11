// Argus content script — DOM layer. Snapshot of visible interactive elements
// with CSS selector paths. Password fields are never read or filled here.

(() => {
  if (window.__argusSnapshot) return;

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
      'a, button, input, textarea, select, summary, [role="button"], [role="tab"], [role="search"], [role="combobox"], [role="switch"], [onclick], [aria-expanded], [contenteditable="true"]';
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
