import { useProviderLogo } from "../../hooks/useProviderLogo";
import type { Provider } from "../../lib/ipc";
import { Btn } from "./kit";

type Props = {
  providers: Provider[];
  onDisconnect?: (id: string) => void;
  onConnect?: (id: string) => void;
};

function ProviderLogo({ id, name }: { id: string; name: string }) {
  const uri = useProviderLogo(id);

  if (uri) {
    return (
      <span className="logo-plate flex h-7 w-7 shrink-0 items-center justify-center rounded-md">
        <img src={uri} alt="" className="h-5 w-5 object-contain" />
      </span>
    );
  }

  const initials = name
    .split(/[\s-]+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  return (
    <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-bg-hover-secondary text-[10px] font-medium leading-none text-text-secondary">
      {initials}
    </div>
  );
}

// Rows only, the Card draws the box. A badge repeating the button is noise.
export default function ConnectedProviderList({
  providers,
  onDisconnect,
  onConnect,
}: Props) {
  return (
    <>
      {providers.map((p) => (
        <div
          key={p.id}
          className="flex items-center gap-4 px-4 py-3.5"
        >
          <ProviderLogo id={p.id} name={p.name} />
          <span className="min-w-0 flex-1 truncate text-sm text-text-primary">
            {p.name}
          </span>
          {p.connected ? (
            <Btn onClick={() => onDisconnect?.(p.id)}>Disconnect</Btn>
          ) : (
            <Btn onClick={() => onConnect?.(p.id)}>Connect</Btn>
          )}
        </div>
      ))}
    </>
  );
}
