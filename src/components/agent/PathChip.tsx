import { openPath } from "@tauri-apps/plugin-opener";
import { CiFileOn, CiFolderOn } from "react-icons/ci";

type Props = { path: string };

export default function PathChip({ path }: Props) {
  const base = path.split("/").filter(Boolean).pop() ?? path;
  const isFile = base.includes(".");

  const open = () => {
    openPath(path).catch(() => {});
  };

  const Icon = isFile ? CiFileOn : CiFolderOn;

  return (
    <button
      type="button"
      onClick={open}
      title={path}
      className={
        "inline-flex items-center gap-1 rounded border border-border-primary " +
        "bg-bg-secondary px-1.5 py-0.5 align-middle font-sans text-xs " +
        "font-medium text-text-secondary transition-colors " +
        "hover:bg-bg-hover-primary hover:text-text-primary focus:outline-none"
      }
    >
      <Icon size={13} className="shrink-0" />
      {base}
    </button>
  );
}
