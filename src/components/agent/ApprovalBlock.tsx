import { useState } from "react";
import { FaGithub } from "react-icons/fa";
import { FiShield } from "react-icons/fi";

import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

const TYPE_LABEL: Record<string, string> = {
  git_push: "Git push",
  payment: "Payment",
  send_email: "Send email",
  delete: "Delete",
  login: "Login",
  force_push: "Force push",
};

export default function ApprovalBlock({ block }: Props) {
  const [decision, setDecision] = useState<
    "pending" | "approved" | "allowed_once" | "allowed_always" | "rejected"
  >("pending");
  const type = block.attrs.type ?? "approval";
  const body = block.children.map((child) => child.value).join("").trim();
  const isGithub = type === "git_push" || type === "force_push";

  if (decision !== "pending") {
    return (
      <div
        data-component-id={block.attrs.id}
        className={
          "flex flex-col gap-2 rounded-lg border border-border-primary " +
          "bg-bg-secondary p-3 font-sans"
        }
      >
        <div className="flex items-center gap-2 text-text-secondary">
          {isGithub ? <FaGithub size={14} /> : <FiShield size={12} />}
          <span className="text-xs font-medium">
            {TYPE_LABEL[type] ?? type} — approval needed
          </span>
        </div>
        {body && <div className="text-sm text-text-primary">{body}</div>}
        <div className="text-xs font-medium text-text-secondary">
          {decisionLabel(decision)}
        </div>
      </div>
    );
  }

  if (isGithub) {
    return (
      <div
        data-component-id={block.attrs.id}
        className={
          "flex flex-col gap-2 rounded-lg border border-border-primary " +
          "bg-bg-secondary p-3 font-sans"
        }
      >
        <div className="flex items-center gap-2 text-text-secondary">
          <FaGithub size={14} />
          <span className="text-xs font-medium">
            {TYPE_LABEL[type] ?? type} — approval needed
          </span>
        </div>
        {body && <div className="text-sm text-text-primary">{body}</div>}
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={() => setDecision("allowed_always")}
            className={
              "rounded-md bg-white px-2.5 py-1 text-xs font-medium " +
              "text-bg-primary transition-opacity hover:opacity-90 " +
              "focus:outline-none"
            }
          >
            Allow always
          </button>
          <button
            type="button"
            onClick={() => setDecision("allowed_once")}
            className={
              "rounded-md border border-border-primary bg-transparent " +
              "px-2.5 py-1 text-xs font-medium text-text-secondary " +
              "transition-colors hover:bg-bg-hover-primary " +
              "hover:text-text-primary focus:outline-none " +
              "focus-visible:bg-bg-hover-primary"
            }
          >
            Allow once
          </button>
          <button
            type="button"
            onClick={() => setDecision("rejected")}
            className={
              "rounded-md border border-border-primary px-2.5 py-1 " +
              "text-xs font-medium text-text-secondary " +
              "transition-colors hover:bg-bg-hover-primary " +
              "hover:text-text-primary focus:outline-none " +
              "focus-visible:bg-bg-hover-primary"
            }
          >
            Reject
          </button>
        </div>
      </div>
    );
  }

  return (
    <div
      data-component-id={block.attrs.id}
      className={
        "flex flex-col gap-2 rounded-lg border border-border-primary " +
        "bg-bg-secondary p-3 font-sans"
      }
    >
      <div className="flex items-center gap-2 text-text-secondary">
        <FiShield size={12} />
        <span className="text-xs font-medium">
          {TYPE_LABEL[type] ?? type} — approval needed
        </span>
      </div>
      {body && <div className="text-sm text-text-primary">{body}</div>}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          onClick={() => setDecision("approved")}
          className={
            "rounded-md bg-white px-2.5 py-1 text-xs font-medium " +
            "text-bg-primary transition-opacity hover:opacity-90 " +
            "focus:outline-none"
          }
        >
          Approve
        </button>
        <button
          type="button"
          onClick={() => setDecision("rejected")}
          className={
            "rounded-md border border-border-primary px-2.5 py-1 " +
            "text-xs font-medium text-text-secondary " +
            "transition-colors hover:bg-bg-hover-primary " +
            "hover:text-text-primary focus:outline-none " +
            "focus-visible:bg-bg-hover-primary"
          }
        >
          Reject
        </button>
      </div>
    </div>
  );
}

function decisionLabel(
  decision: "approved" | "allowed_once" | "allowed_always" | "rejected",
): string {
  if (decision === "allowed_always") return "Allowed always";
  if (decision === "allowed_once") return "Allowed once";
  if (decision === "approved") return "Approved";

  return "Rejected";
}
