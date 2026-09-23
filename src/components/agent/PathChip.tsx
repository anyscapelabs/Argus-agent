import { openPath } from "@tauri-apps/plugin-opener";

type Props = { path: string };

export default function PathChip({ path }: Props) {
  const open = () => {
    openPath(path).catch(() => {});
  };

  return (
    <button
      type="button"
      onClick={open}
      title={path}
      className={
        "break-all cursor-pointer text-blue-400 underline " +
        "decoration-blue-400/30 underline-offset-2 hover:text-blue-300 " +
        "focus:outline-none"
      }
    >
      {path}
    </button>
  );
}
