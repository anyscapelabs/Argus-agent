import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function DiffBlock({ block }: Props) {
  const file = block.attrs.file ?? "untitled";
  const language = block.attrs.language;
  const raw = block.children.map((child) => child.value).join("");
  const lines = raw.split("\n");

  return (
    <div
      className={
        "mt-4 overflow-hidden rounded-lg border border-border-primary " +
        "bg-bg-secondary font-sans first:mt-0"
      }
    >
      <div className="flex items-center gap-2 border-b border-border-primary px-3 py-1.5 text-xs text-text-secondary">
        <span className="font-mono">{file}</span>
        {language && (
          <span className="rounded-full border border-border-primary px-1.5 py-0.5 text-[10px]">
            {language}
          </span>
        )}
      </div>
      <pre className="overflow-x-auto p-3 font-mono text-xs leading-5 text-text-primary">
        {lines.map((line, index) => {
          const isAdd = line.startsWith("+");
          const isRemove = line.startsWith("-");
          const className = isAdd
            ? "text-emerald-300"
            : isRemove
              ? "text-red-300"
              : "text-text-primary";

          return (
            <div key={index} className={`whitespace-pre ${className}`}>
              {line || " "}
            </div>
          );
        })}
      </pre>
    </div>
  );
}
