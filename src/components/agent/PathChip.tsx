import { openPath } from "@tauri-apps/plugin-opener";

type Props = { path: string };

export default function PathChip({ path }: Props) {
  const base = path.split("/").filter(Boolean).pop() ?? path;

  const open = () => {
    openPath(path).catch(() => {});
  };

  return (
    <button
      type="button"
      onClick={open}
      title={path}
      className={
        "inline-flex items-center rounded align-middle text-sm font-medium leading-none " +
        "text-blue-400 hover:text-blue-300 hover:underline focus:outline-none"
      }
    >
      {base}
    </button>
  );
}
