import { FiAlertCircle, FiCheckCircle, FiInfo, FiX } from "react-icons/fi";

import { toast, useToasts, type ToastKind } from "../stores/toast";

function iconFor(kind: ToastKind) {
  if (kind === "success")
    return <FiCheckCircle size={15} className="shrink-0 text-emerald-400" />;
  if (kind === "error")
    return <FiAlertCircle size={15} className="shrink-0 text-red-400" />;
  return <FiInfo size={15} className="shrink-0 text-text-secondary" />;
}

export default function Toasts() {
  const items = useToasts();

  return (
    <div
      aria-live="polite"
      className="pointer-events-none fixed inset-x-0 bottom-6 z-[60] flex flex-col items-center gap-2 px-4"
    >
      {items.map((item) => (
        <div
          key={item.id}
          className={
            "toast-in pointer-events-auto flex max-w-md items-center gap-2.5 " +
            "rounded-xl border border-border-primary bg-bg-secondary px-3.5 py-2.5 " +
            "shadow-4xl"
          }
        >
          {iconFor(item.kind)}
          <span className="min-w-0 flex-1 truncate text-sm font-medium text-text-primary">
            {item.msg}
          </span>
          <button
            type="button"
            onClick={() => toast.dismiss(item.id)}
            aria-label="Dismiss"
            className={
              "flex h-6 w-6 shrink-0 items-center justify-center rounded-md " +
              "text-text-secondary transition-colors hover:bg-bg-hover-primary " +
              "hover:text-text-primary focus:outline-none"
            }
          >
            <FiX size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}
