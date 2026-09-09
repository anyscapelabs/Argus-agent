import type { BlockNode } from "../../lib/agentXml";

type Props = { block: BlockNode };

export default function TableBlock({ block }: Props) {
  const raw = block.children.map((c) => c.value).join("");
  if (!raw.trim()) return null;

  const rowRe = /<tr[^>]*>([\s\S]*?)<\/tr>/gi;
  const rows: { cells: { txt: string; isHead: boolean }[] }[] = [];
  let m: RegExpExecArray | null;

  while ((m = rowRe.exec(raw)) !== null) {
    const rowRaw = m[1];
    const cellRe = /<(th|td)[^>]*>([\s\S]*?)<\/\1>/gi;
    const cells: { txt: string; isHead: boolean }[] = [];
    let c: RegExpExecArray | null;

    while ((c = cellRe.exec(rowRaw)) !== null) {
      const tag = c[1].toLowerCase();
      const txt = c[2].trim();
      cells.push({ txt, isHead: tag === "th" });
    }

    if (cells.length) rows.push({ cells });
  }

  if (!rows.length) return null;

  const headRow = rows[0].cells.every((c) => c.isHead) ? rows[0] : null;
  const bodyRows = headRow ? rows.slice(1) : rows;

  return (
    <div className="mt-4 overflow-hidden rounded-lg border border-border-primary first:mt-0">
      <table className="w-full border-collapse text-sm">
        {headRow && (
          <thead>
            <tr className="bg-bg-secondary">
              {headRow.cells.map((cell, idx) => (
                <th
                  key={idx}
                  className="px-3 py-2 text-left font-medium text-text-primary"
                >
                  {cell.txt}
                </th>
              ))}
            </tr>
          </thead>
        )}
        <tbody>
          {bodyRows.map((row, rIdx) => (
            <tr key={rIdx} className="border-t border-border-primary">
              {row.cells.map((cell, cIdx) => (
                <td
                  key={cIdx}
                  className="px-3 py-2 text-left font-light text-text-secondary"
                >
                  {cell.txt}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
