import { isDesktop } from "../desktop";

const MONTHS = [
  "Janeiro",
  "Fevereiro",
  "Março",
  "Abril",
  "Maio",
  "Junho",
  "Julho",
  "Agosto",
  "Setembro",
  "Outubro",
  "Novembro",
  "Dezembro",
];

type Props = {
  /** YYYY-MM */
  value: string;
  onChange: (value: string) => void;
  /** Latest month that can be chosen (YYYY-MM). */
  max?: string;
  className?: string;
};

/**
 * Month picker. The desktop WebView (WebKit) has no native one, so there it is
 * a month list and a year field.
 */
export default function MonthInput({ value, onChange, max, className }: Props) {
  if (!isDesktop) {
    return (
      <input
        type="month"
        value={value}
        max={max}
        onChange={(e) => onChange(e.target.value)}
        className={`${className} block`}
      />
    );
  }
  const [year, month] = value.split("-");
  const [maxYear, maxMonth] = max?.split("-") ?? [];
  const set = (y: string, m: string) => {
    const next = `${y.padStart(4, "0")}-${m}`;
    onChange(max && next > max ? max : next);
  };
  return (
    <div className="flex gap-2">
      <select
        aria-label="Mês"
        value={month}
        onChange={(e) => set(year, e.target.value)}
        className={className}
      >
        {MONTHS.map((name, i) => {
          const m = String(i + 1).padStart(2, "0");
          return (
            <option
              key={m}
              value={m}
              disabled={year === maxYear && m > maxMonth}
            >
              {name}
            </option>
          );
        })}
      </select>
      <input
        aria-label="Ano"
        type="number"
        min={2000}
        max={maxYear ?? 2100}
        value={Number(year)}
        onChange={(e) =>
          e.target.value.length === 4 && set(e.target.value, month)
        }
        className={`${className} w-24`}
      />
    </div>
  );
}
