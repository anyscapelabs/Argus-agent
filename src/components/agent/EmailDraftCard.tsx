import { FiMail } from "react-icons/fi";
import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function EmailDraftCard({ block }: Props) {
  const to = block.attrs.to ?? "(no recipient)";
  const subject = block.attrs.subject ?? "(no subject)";
  const body = block.children.map((child) => child.value).join("").trim();

  return (
    <div
      data-component-id={block.attrs.id}
      className="flex flex-col gap-2 rounded-lg border border-border-primary bg-bg-secondary p-3 font-sans"
    >
      <div className="flex items-center gap-2 text-xs text-text-secondary">
        <FiMail size={12} />
        <span>Email draft</span>
      </div>
      <div className="flex flex-col gap-0.5 text-sm">
        <div className="flex items-baseline gap-2">
          <span className="w-12 shrink-0 text-text-secondary">To</span>
          <span className="truncate text-text-primary">{to}</span>
        </div>
        <div className="flex items-baseline gap-2">
          <span className="w-12 shrink-0 text-text-secondary">Subject</span>
          <span className="truncate text-text-primary">{subject}</span>
        </div>
      </div>
      {body && (
        <div className="mt-1 whitespace-pre-wrap border-t border-border-primary pt-2 text-sm text-text-primary">
          {body}
        </div>
      )}
      <div className="flex items-center gap-1">
        <button
          type="button"
          className="rounded-md bg-white px-2.5 py-1 text-xs font-medium text-bg-primary transition-opacity hover:opacity-90 focus:outline-none"
        >
          Send
        </button>
        <button
          type="button"
          className="rounded-md border border-border-primary px-2.5 py-1 text-xs font-medium text-text-secondary transition-colors hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none focus-visible:bg-bg-hover-primary"
        >
          Edit
        </button>
      </div>
    </div>
  );
}
