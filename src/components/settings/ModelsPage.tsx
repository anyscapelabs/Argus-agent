import { memo, useMemo, useState } from "react";
import { LuChevronDown, LuSearch } from "react-icons/lu";
import { useModels } from "../../hooks/useModels";
import { useProviders } from "../../hooks/useProviders";
import { useProviderLogo } from "../../hooks/useProviderLogo";
import type { ProviderModel } from "../../lib/ipc";

function Switch({ on, onChange }: { on: boolean; onChange: (next: boolean) => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      onClick={() => onChange(!on)}
      className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
        on ? "bg-accent" : "border border-border-primary bg-bg-hover-secondary"
      }`}
    >
      <span
        className={`absolute top-1/2 left-0.5 h-4 w-4 -translate-y-1/2 rounded-full bg-white shadow transition-transform ${
          on ? "translate-x-4" : ""
        }`}
      />
    </button>
  );
}

function ProviderLogo({ id, name }: { id: string; name: string }) {
  const uri = useProviderLogo(id);

  if (uri) {
    return <img src={uri} alt="" className="h-5 w-5 shrink-0 rounded object-contain p-0.5 invert" />;
  }

  const initials = name
    .split(/[\s-]+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  return (
    <div className="flex h-5 w-5 shrink-0 items-center justify-center rounded bg-accent text-[9px] font-semibold text-bg-primary">
      {initials}
    </div>
  );
}

const fmtPrice = (v: number) => (v >= 0.01 ? v.toFixed(2) : v.toFixed(4));
const perM = (c: number) => (c > 0 ? `$${fmtPrice(c * 1000)}/M tok` : "free");

const ModelRow = memo(function ModelRow({
  model,
  onToggle,
}: {
  model: ProviderModel;
  onToggle: (id: string, on: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border-primary px-2 py-1.5 last:border-b-0 [contain-intrinsic-size:auto_44px] [content-visibility:auto]">
      <div className="min-w-0">
        <p className="truncate text-sm text-text-primary">{model.displayName}</p>
        <p className="truncate text-xs text-text-secondary">
          {model.enabled ? `in chat · ${perM(model.costIn)} in / ${perM(model.costOut)} out` : perM(model.costIn)}
        </p>
      </div>
      <Switch on={model.enabled} onChange={(next) => onToggle(model.modelId, next)} />
    </div>
  );
});

const ModelSection = memo(function ModelSection({
  providerId,
  name,
  models,
  open,
  onToggleOpen,
  onToggleModel,
}: {
  providerId: string;
  name: string;
  models: ProviderModel[];
  open: boolean;
  onToggleOpen: (id: string) => void;
  onToggleModel: (id: string, on: boolean) => void;
}) {
  return (
    <div className="overflow-hidden rounded-xl border border-border-primary">
      <button
        type="button"
        onClick={() => onToggleOpen(providerId)}
        className="flex w-full items-center gap-2 px-2 py-2 text-left transition-colors hover:bg-bg-hover-secondary"
      >
        <ProviderLogo id={providerId} name={name} />
        <span className="truncate text-sm font-medium text-text-primary">{name}</span>
        <span className="text-xs text-text-secondary">
          {models.filter((m) => m.enabled).length}/{models.length}
        </span>
        <LuChevronDown
          size={14}
          className={`ml-auto shrink-0 text-text-secondary transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div className="border-t border-border-primary">
          {models.map((m) => (
            <ModelRow key={m.modelId} model={m} onToggle={onToggleModel} />
          ))}
        </div>
      )}
    </div>
  );
});

export default function ModelsPage() {
  const { groups, loading, err, query, setQuery, toggle } = useModels();
  const { providers } = useProviders();
  const [open, setOpen] = useState<Set<string>>(new Set());

  const connectedIds = useMemo(
    () => new Set(providers.filter((p) => p.connected).map((p) => p.id)),
    [providers],
  );
  const visible = groups.filter((g) => connectedIds.has(g.providerId));
  const searching = query.trim().length > 0;

  const isOpen = (providerId: string) => (searching ? true : open.has(providerId));
  const toggleOpen = (providerId: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(providerId)) {
        next.delete(providerId);
      } else {
        next.add(providerId);
      }
      return next;
    });

  return (
    <div className="flex flex-col gap-6">
      <h2 className="text-sm font-semibold text-text-primary">Models</h2>

      <div className="relative">
        <LuSearch size={14} className="absolute top-1/2 left-3 -translate-y-1/2 text-text-secondary" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search models…"
          className="w-full rounded-lg border border-border-primary bg-bg-hover-secondary py-1.5 pr-3 pl-8 text-sm text-text-primary placeholder:text-text-secondary focus:border-text-secondary focus:outline-none focus:ring-1 focus:ring-text-secondary"
        />
      </div>

      {loading ? (
        <p className="text-sm text-text-secondary">Loading models…</p>
      ) : err ? (
        <p className="text-sm text-red-400">{err}</p>
      ) : visible.length === 0 ? (
        <p className="text-sm text-text-secondary">
          No models to show — connect a provider on the Providers page, then sync the catalog.
        </p>
      ) : (
        <div className="flex flex-col gap-2">
          {visible.map(({ providerId, models }) => (
            <ModelSection
              key={providerId}
              providerId={providerId}
              name={providers.find((p) => p.id === providerId)?.name ?? providerId}
              models={models}
              open={isOpen(providerId)}
              onToggleOpen={toggleOpen}
              onToggleModel={toggle}
            />
          ))}
        </div>
      )}
    </div>
  );
}
