import { memo, useMemo, useState, type ReactNode } from "react";
import {
  LuBoxes,
  LuChevronDown,
  LuPlug,
  LuSearch,
  LuSearchX,
} from "react-icons/lu";

import { useModels } from "../../hooks/useModels";
import { useProviderLogo } from "../../hooks/useProviderLogo";
import { useProviders } from "../../hooks/useProviders";
import type { ProviderModel } from "../../lib/ipc";

type SwitchProps = {
  on: boolean;
  onChange: (next: boolean) => void;
};

function Switch({ on, onChange }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      onClick={() => onChange(!on)}
      className={`relative h-5 w-9 shrink-0 rounded-full transition-colors ${
        on
          ? "bg-accent"
          : "border border-border-primary bg-bg-hover-secondary"
      }`}
    >
      <span
        className={`absolute top-1/2 left-0.5 h-4 w-4 -translate-y-1/2 rounded-full shadow transition-transform ${
          on ? "translate-x-4 bg-bg-primary" : "bg-white"
        }`}
      />
    </button>
  );
}

type ProviderLogoProps = {
  id: string;
  name: string;
};

function ProviderLogo({ id, name }: ProviderLogoProps) {
  const uri = useProviderLogo(id);

  if (uri) {
    return (
      <img
        src={uri}
        alt=""
        className="h-5 w-5 shrink-0 rounded object-contain p-0.5 invert"
      />
    );
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

type ModelRowProps = {
  model: ProviderModel;
  onToggle: (id: string, on: boolean) => void;
};

const ModelRow = memo(function ModelRow({
  model,
  onToggle,
}: ModelRowProps) {
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border-primary px-2 py-1.5 last:border-b-0 [contain-intrinsic-size:auto_40px] [content-visibility:auto]">
      <p className="min-w-0 truncate text-sm text-text-primary">
        {model.displayName}
      </p>
      <Switch
        on={model.enabled}
        onChange={(next) => onToggle(model.modelId, next)}
      />
    </div>
  );
});

type ModelSectionProps = {
  providerId: string;
  name: string;
  models: ProviderModel[];
  open: boolean;
  onToggleOpen: (id: string) => void;
  onToggleModel: (id: string, on: boolean) => void;
};

const ModelSection = memo(function ModelSection({
  providerId,
  name,
  models,
  open,
  onToggleOpen,
  onToggleModel,
}: ModelSectionProps) {
  return (
    <div className="overflow-hidden rounded-xl border border-border-primary">
      <button
        type="button"
        onClick={() => onToggleOpen(providerId)}
        className="flex w-full items-center gap-2 px-2 py-2 text-left transition-colors hover:bg-bg-hover-secondary"
      >
        <ProviderLogo id={providerId} name={name} />
        <span className="truncate text-sm font-medium text-text-primary">
          {name}
        </span>
        <span className="text-xs text-text-secondary">
          {models.filter((m) => m.enabled).length}/{models.length}
        </span>
        <LuChevronDown
          size={14}
          className={`ml-auto shrink-0 text-text-secondary transition-transform ${
            open ? "rotate-180" : ""
          }`}
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

type EmptyStateProps = {
  icon: ReactNode;
  title: string;
  body: string;
  action?: ReactNode;
};

function EmptyState({ icon, title, body, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center gap-3 px-6 py-12 text-center">
      <div className="flex h-10 w-10 items-center justify-center rounded-full bg-bg-hover-secondary text-text-secondary">
        {icon}
      </div>
      <div className="flex flex-col gap-1">
        <p className="text-sm font-medium text-text-primary">{title}</p>
        <p className="max-w-xs text-xs leading-relaxed text-text-secondary">
          {body}
        </p>
      </div>
      {action}
    </div>
  );
}

type ModelsPageProps = {
  onNavigate?: (tab: "providers" | "models") => void;
};

export default function ModelsPage({ onNavigate }: ModelsPageProps) {
  const { groups, loading, err, query, setQuery, toggle } = useModels();
  const {
    providers,
    loading: provLoading,
    syncing,
    syncCatalog,
  } = useProviders();
  const [open, setOpen] = useState<Set<string>>(new Set());

  const connectedIds = useMemo(
    () => new Set(providers.filter((p) => p.connected).map((p) => p.id)),
    [providers],
  );

  const visible = groups.filter((g) => connectedIds.has(g.providerId));
  const searching = query.trim().length > 0;

  function isOpen(providerId: string): boolean {
    if (searching) return true;
    return open.has(providerId);
  }

  function toggleOpen(providerId: string): void {
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(providerId)) {
        next.delete(providerId);
        return next;
      }

      next.add(providerId);
      return next;
    });
  }

  return (
    <div className="flex flex-col gap-6">
      <h2 className="text-sm font-semibold text-text-primary">Models</h2>

      <div className="relative">
        <LuSearch
          size={14}
          className="absolute top-1/2 left-3 -translate-y-1/2 text-text-secondary"
        />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search models…"
          className="w-full rounded-lg border border-border-primary bg-bg-hover-secondary py-1.5 pr-3 pl-8 text-sm text-text-primary placeholder:text-text-secondary focus:border-text-secondary focus:outline-none focus:ring-1 focus:ring-text-secondary"
        />
      </div>

      {loading || provLoading ? (
        <p className="text-sm text-text-secondary">Loading models…</p>
      ) : err ? (
        <p className="text-sm text-red-400">{err}</p>
      ) : visible.length === 0 && searching ? (
        <EmptyState
          icon={<LuSearchX size={18} />}
          title="No models match"
          body={`Nothing found for "${query.trim()}". Try a shorter or different name.`}
          action={
            <button
              type="button"
              onClick={() => setQuery("")}
              className="rounded-full border border-border-primary bg-bg-hover-secondary px-4 py-1.5 text-sm font-medium text-text-primary transition-colors hover:bg-bg-hover-primary"
            >
              Clear search
            </button>
          }
        />
      ) : visible.length === 0 && connectedIds.size === 0 ? (
        <EmptyState
          icon={<LuPlug size={18} />}
          title="No connected providers"
          body="Models come from your connected providers. Connect one first, then sync the catalog to pull its models."
          action={
            <button
              type="button"
              onClick={() => onNavigate?.("providers")}
              className="rounded-full border border-border-primary bg-bg-hover-secondary px-4 py-1.5 text-sm font-medium text-text-primary transition-colors hover:bg-bg-hover-primary"
            >
              Go to Providers
            </button>
          }
        />
      ) : visible.length === 0 ? (
        <EmptyState
          icon={<LuBoxes size={18} />}
          title="No models yet"
          body="Your connected providers have no models indexed. Sync the catalog to fetch the latest list."
          action={
            <button
              type="button"
              onClick={syncCatalog}
              disabled={syncing}
              className="flex items-center gap-1.5 rounded-full border border-border-primary bg-bg-hover-secondary px-4 py-1.5 text-sm font-medium text-text-primary transition-colors hover:bg-bg-hover-primary disabled:opacity-50"
            >
              {syncing ? "Syncing…" : "Sync catalog"}
            </button>
          }
        />
      ) : (
        <div className="flex flex-col gap-2">
          {visible.map(({ providerId, models }) => (
            <ModelSection
              key={providerId}
              providerId={providerId}
              name={
                providers.find((p) => p.id === providerId)?.name ??
                providerId
              }
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
