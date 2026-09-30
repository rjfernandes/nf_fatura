import type { ReactNode } from "react";

export type TableHeader<T> = {
  label: ReactNode;
  /**
   * Row attribute name, or a function receiving the row and returning what to
   * render.
   */
  field: keyof T | ((row: T) => ReactNode);
  /**
   * Extra classes applied to both the <th> and the <td> of this column (e.g.
   * "text-right").
   */
  className?: string;
};

type TableProps<T> = {
  headers: TableHeader<T>[];
  rows: T[];
  rowKey: (row: T) => string;
  emptyMessage?: ReactNode;
  onRowClick?: (row: T) => void;
  rowClassName?: string;
  /** Tailwind padding class for cells. */
  cellPadding?: string;
  /** Classes for the <thead>. */
  headClassName?: string;
};

function renderCell<T>(row: T, field: TableHeader<T>["field"]): ReactNode {
  if (typeof field === "function") {
    return field(row);
  }
  return row[field] as ReactNode;
}

export default function Table<T>({
  headers,
  rows,
  rowKey,
  emptyMessage,
  onRowClick,
  rowClassName = "",
  cellPadding = "p-3",
  headClassName = "bg-slate-100",
}: TableProps<T>) {
  return (
    <table className="w-full text-left text-sm">
      <thead className={`text-xs uppercase text-slate-500 ${headClassName}`}>
        <tr>
          {headers.map((h, i) => (
            <th key={i} className={`${cellPadding} ${h.className ?? ""}`}>
              {h.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && emptyMessage && (
          <tr>
            <td
              colSpan={headers.length}
              className={`${cellPadding} text-slate-500`}
            >
              {emptyMessage}
            </td>
          </tr>
        )}
        {rows.map((row) => (
          <tr
            key={rowKey(row)}
            className={`border-t border-slate-100 ${onRowClick ? "cursor-pointer hover:bg-slate-50" : ""} ${rowClassName}`}
            onClick={onRowClick && (() => onRowClick(row))}
          >
            {headers.map((h, i) => (
              <td key={i} className={`${cellPadding} ${h.className ?? ""}`}>
                {renderCell(row, h.field)}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
