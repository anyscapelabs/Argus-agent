import { useState } from "react";
import { FiCheck, FiCopy } from "react-icons/fi";

export default function CodeBlock({
  lang,
  code,
}: {
  lang: string;
  code: string;
}) {
  const [copied, setCopied] = useState(false);
  const [wrap, setWrap] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {}
  };

  return (
    <div className="mt-2 overflow-hidden rounded-lg border border-border-primary first:mt-0">
      <div className="flex items-center justify-between border-b border-border-primary px-3 py-1">
        <span className="font-mono text-[10px] text-text-secondary">
          {lang !== "" ? lang : "code"}
        </span>
        <span className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setWrap((v) => !v)}
            className="font-mono text-[10px] text-text-secondary hover:text-text-primary cursor-pointer"
          >
            {wrap ? "No wrap" : "Wrap"}
          </button>
          <button
            type="button"
            onClick={copy}
            aria-label="Copy code"
            className="flex items-center gap-1 font-mono text-[10px] text-text-secondary hover:text-text-primary cursor-pointer"
          >
            {copied ? <FiCheck size={12} /> : <FiCopy size={12} />}
            {copied ? "Copied" : "Copy"}
          </button>
        </span>
      </div>
      <pre
        className={
          "overflow-x-auto px-3 py-2 font-mono text-xs leading-5 text-text-primary " +
          `${wrap ? "whitespace-pre-wrap break-all" : ""}`
        }
      >
        {code}
      </pre>
    </div>
  );
}
