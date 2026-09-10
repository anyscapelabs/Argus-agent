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
        "inline-flex items-center gap-1 rounded text-sm font-medium " +
        "text-blue-400 hover:text-blue-300 hover:underline focus:outline-none"
      }
    >
      <Icon size={14} className="shrink-0" />
      {base}
    </button>
  );
}
