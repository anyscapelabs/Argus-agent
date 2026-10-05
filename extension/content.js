(() => {
  if (window.__argusJsV === 5) return;
  window.__argusJsV = 5;

  window.__argusResolve = (p) => {
    if (typeof p === "string" && p.startsWith("shadow:")) {
      let hops = [];
      try {
        hops = JSON.parse(p.slice(7));
      } catch (e) {
        return null;
      }
      let node = document;
      for (const h of hops) {
        if (!node || !node.querySelector) return null;
        let el = null;
        try {
          el = node.querySelector(h.s);
        } catch (e) {
          return null;
        }
        if (!el) return null;
        if (h.via === "shadow") {
          node = el.shadowRoot;
          if (!node) return null;
        } else if (h.via === "frame") {
          try {
            node = el.contentDocument;
          } catch (e) {
            return null;
          }
          if (!node) return null;
        } else {
          node = el;
        }
      }
      return node && node.tagName ? node : null;
    }
    try {
      return document.querySelector(p);
    } catch (e) {
      return null;
    }
  };

  function selFor(el, scope) {
    let path = "";

    for (let n = el; n && n.nodeType === 1 && n !== document.body && n !== scope; n = n.parentNode) {
      if (n.id) {
        path = `#${CSS.escape(n.id)}${path ? " > " + path : ""}`;
        break;
      }

      const p = n.parentNode;
      let s = n.tagName.toLowerCase();

      if (p && p.children) {
        const sib = [...p.children].filter((c) => c.tagName === n.tagName);
        if (sib.length > 1) s += `:nth-of-type(${sib.indexOf(n) + 1})`;
      }

      path = path ? `${s} > ${path}` : s;
    }

    return path;
  }

  window.__argusSnapshot = () => {
    const sel =
      'a, button, input, textarea, select, summary, [role="button"], [role="tab"], [role="search"], [role="combobox"], [role="switch"], [role="option"], [draggable="true"], [onclick], [aria-expanded], [contenteditable="true"]';
    const out = [];

    function pushEl(el, hops, scope) {
      const r = el.getBoundingClientRect();

      if (r.width === 0 || r.height === 0) return;
      if (el.disabled) return;

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

      const tail = selFor(el, scope);
      if (!tail) return;
      const path =
        hops.length === 0 ? tail : "shadow:" + JSON.stringify([...hops, { s: tail }]);

      out.push({
        kind:
          el.tagName.toLowerCase() === "input" && el.type
            ? `input ${el.type}`
            : el.tagName.toLowerCase(),
        label,
        path,
      });
    }

    function collect(root, hops, depth) {
      if (out.length >= 100 || depth > 4) return;

      for (const el of root.querySelectorAll(sel)) {
        if (out.length >= 100) return;
        pushEl(el, hops, root);
      }

      if (depth >= 4) return;

      let all = [];
      try {
        all = [...root.querySelectorAll("*")];
      } catch (e) {
        all = [];
      }

      for (const el of all) {
        if (out.length >= 100) return;

        const hs = selFor(el, root);
        if (!hs) continue;

        if (el.shadowRoot) {
          collect(el.shadowRoot, [...hops, { s: hs, via: "shadow" }], depth + 1);
        }

        if (el.tagName === "IFRAME") {
          let doc = null;
          try {
            doc = el.contentDocument;
          } catch (e) {
            doc = null;
          }
          if (doc) {
            collect(doc, [...hops, { s: hs, via: "frame" }], depth + 1);
          }
        }
      }
    }

    collect(document, [], 0);

    return out;
  };
})();
