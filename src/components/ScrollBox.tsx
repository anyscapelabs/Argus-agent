import { useLayoutEffect, useRef, useState, type ReactNode } from "react";

const THUMB_MIN = 24;

type Props = {
  children: ReactNode;
  className?: string;
};

export default function ScrollBox({ children, className }: Props) {
  const listRef = useRef<HTMLDivElement | null>(null);
  const thumbRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{ y: number; st: number } | null>(null);
  const [barOn, setBarOn] = useState(false);

  const sync = () => {
    const el = listRef.current;
    const thumb = thumbRef.current;

    if (!el || !thumb) return;

    const overflow = el.scrollHeight > el.clientHeight;
    setBarOn((prev) => (prev === overflow ? prev : overflow));

    if (!overflow) return;

    const vis = el.clientHeight;
    const h = Math.max((vis / el.scrollHeight) * vis, THUMB_MIN);
    const top = (el.scrollTop / el.scrollHeight) * vis;

    thumb.style.height = `${h}px`;
    thumb.style.top = `${top + 2}px`;
  };

  useLayoutEffect(() => {
    sync();
  }, [children]);

  const onThumbDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const el = listRef.current;

    if (!el) return;

    dragRef.current = { y: e.clientY, st: el.scrollTop };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const onThumbMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const el = listRef.current;
    const d = dragRef.current;

    if (!el || !d) return;

    el.scrollTop =
      d.st + (e.clientY - d.y) * (el.scrollHeight / el.clientHeight);
  };

  const onThumbUp = () => {
    dragRef.current = null;
  };

  return (
    <div className={"relative " + (className ?? "")}>
      <div
        ref={listRef}
        onScroll={sync}
        className="h-full overflow-y-auto overscroll-contain"
      >
        {children}
      </div>
      {barOn && (
        <div
          ref={thumbRef}
          onPointerDown={onThumbDown}
          onPointerMove={onThumbMove}
          onPointerUp={onThumbUp}
          className="absolute right-[3px] top-2 h-6 w-[5px] cursor-default rounded-full bg-[#3f3f3f]"
        />
      )}
    </div>
  );
}
