import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

const DEF_FILE = "untitled";
const ADD_CLS = "text-emerald-300";
const DEL_CLS = "text-red-300";
const TXT_CLS = "text-text-primary";
const BLANK_LN = " ";

export default function DiffBlock({ block }: Props) {
  const file = block.attrs.file ?? DEF_FILE;
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
        {lines.map((line, idx) => {
          const isAdd = line.startsWith("+");
          if (isAdd) {
            return (
              <div key={idx} className={`whitespace-pre ${ADD_CLS}`}>
                {line || BLANK_LN}
              </div>
            );
          }

          if (line.startsWith("-")) {
            return (
              <div key={idx} className={`whitespace-pre ${DEL_CLS}`}>
                {line || BLANK_LN}
              </div>
            );
          }

          return (
            <div key={idx} className={`whitespace-pre ${TXT_CLS}`}>
              {line || BLANK_LN}
            </div>
          );
        })}
      </pre>
    </div>
  );
}
