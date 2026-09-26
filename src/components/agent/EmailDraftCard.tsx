import { useState } from "react";
import { FiMail } from "react-icons/fi";

const FIELD =
  "w-full rounded-md border border-border-primary bg-bg-primary px-2 py-1 " +
  "text-sm text-text-primary placeholder:text-text-secondary " +
  "focus:outline-none focus:ring-1 focus:ring-text-secondary";

type Props = {
  to: string;
  subject: string;
  body: string;
  onSend?: (draft: { to: string; subject: string; body: string }) => void;
  onDiscard?: () => void;
};

export default function EmailDraftCard({
  to,
  subject,
  body,
  onSend,
  onDiscard,
}: Props) {
  const [toV, setToV] = useState(to);
  const [subjectV, setSubjectV] = useState(subject);
  const [bodyV, setBodyV] = useState(body);
  const [touched, setTouched] = useState(false);

  const val = (edited: string, orig: string) => (touched ? edited : orig);
  const canSend = onSend !== undefined;

  return (
    <div
      className={
        "my-1 flex flex-col gap-2 rounded-lg border border-border-primary " +
        "bg-bg-secondary p-3 font-sans"
      }
    >
      <div className="flex items-center gap-2 text-xs text-text-secondary">
        <FiMail size={12} />
        <span>{canSend ? "Email draft — review before sending" : "Email sent"}</span>
      </div>
      <div className="flex flex-col gap-1.5">
        <label className="flex items-baseline gap-2">
          <span className="w-12 shrink-0 text-xs text-text-secondary">To</span>
          <input
            type="text"
            value={val(toV, to)}
            readOnly={!canSend}
            onChange={(e) => {
              setTouched(true);
              setToV(e.target.value);
            }}
            className={FIELD}
            autoComplete="off"
          />
        </label>
        <label className="flex items-baseline gap-2">
          <span className="w-12 shrink-0 text-xs text-text-secondary">Subject</span>
          <input
            type="text"
            value={val(subjectV, subject)}
            readOnly={!canSend}
            onChange={(e) => {
              setTouched(true);
              setSubjectV(e.target.value);
            }}
            className={FIELD}
            autoComplete="off"
          />
        </label>
        <textarea
          value={val(bodyV, body)}
          readOnly={!canSend}
          rows={Math.min(14, Math.max(3, body.split("\n").length + 1))}
          onChange={(e) => {
            setTouched(true);
            setBodyV(e.target.value);
          }}
          className={FIELD + " resize-y leading-relaxed"}
        />
      </div>
      {canSend && onDiscard && (
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => onSend({ to: toV, subject: subjectV, body: bodyV })}
            className={
              "rounded-md bg-text-primary px-2.5 py-1 text-xs font-medium " +
              "text-bg-primary transition-opacity hover:opacity-90 " +
              "focus:outline-none"
            }
          >
            Send
          </button>
          <button
            type="button"
            onClick={onDiscard}
            className={
              "rounded-md border border-border-primary px-2.5 py-1 text-xs " +
              "font-medium text-text-secondary transition-colors " +
              "hover:bg-bg-hover-primary hover:text-text-primary " +
              "focus:outline-none focus-visible:bg-bg-hover-primary"
            }
          >
            Discard
          </button>
        </div>
      )}
    </div>
  );
}
