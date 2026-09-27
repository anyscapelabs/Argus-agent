import { HiXMark } from "react-icons/hi2";
import { FiFileText, FiImage } from "react-icons/fi";

import {
  attachStore,
  useAttachments,
  type Pending,
} from "../stores/attachments";

function sz(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function Chip({ item }: { item: Pending }) {
  const Icon = item.src !== null ? FiImage : FiFileText;

  return (
    <div
      className="group flex max-w-[180px] items-center gap-1.5 rounded-lg border border-border-primary bg-bg-tertiary py-1 pl-1 pr-1.5"
      title={item.name}
    >
      {item.src !== null ? (
        <img
          src={item.src}
          alt=""
          className="h-7 w-7 shrink-0 rounded object-cover"
        />
      ) : (
        <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded bg-bg-hover-secondary">
          <Icon size={14} className="text-text-secondary" />
        </span>
      )}

      <span className="flex min-w-0 flex-col">
        <span className="truncate text-xs font-medium text-text-primary">
          {item.name}
        </span>
        <span className="truncate text-[10px] text-text-tertiary">
          {sz(item.sz)}
        </span>
      </span>

      <button
        type="button"
        aria-label={`Remove ${item.name}`}
        onClick={() => attachStore.remove(item.id)}
        className="shrink-0 rounded p-0.5 text-text-tertiary opacity-0 transition-opacity hover:bg-bg-hover-secondary hover:text-text-primary focus:opacity-100 focus:outline-none group-hover:opacity-100"
      >
        <HiXMark size={12} />
      </button>
    </div>
  );
}

export default function AttachChips() {
  const { items, err, busy } = useAttachments();

  if (items.length === 0 && err === null && !busy) {
    return null;
  }

  return (
    <div className="mb-1 flex flex-col gap-1">
      {items.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {items.map((item) => (
            <Chip key={item.id} item={item} />
          ))}
        </div>
      )}

      {busy && (
        <span className="px-1 text-xs text-text-secondary">Adding…</span>
      )}

      {err !== null && (
        <span className="px-1 text-xs text-text-secondary">{err}</span>
      )}
    </div>
  );
}
