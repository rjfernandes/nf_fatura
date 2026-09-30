const isoDay = (d: Date) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** First and last day (YYYY-MM-DD) of a YYYY-MM competence. */
export function monthRange(competence: string): { from: string; to: string } {
  const [y, m] = competence.split("-").map(Number);
  const last = new Date(Date.UTC(y, m, 0)).getUTCDate();
  const mm = String(m).padStart(2, "0");
  return {
    from: `${y}-${mm}-01`,
    to: `${y}-${mm}-${String(last).padStart(2, "0")}`,
  };
}

/**
 * Same as monthRange, but the end never goes past today (the current month is
 * still open).
 */
export function monthRangeUntilToday(
  competence: string,
  today = new Date(),
): { from: string; to: string } {
  const { from, to } = monthRange(competence);
  const t = isoDay(today);
  return { from, to: to > t ? t : to };
}

/** True when the month has not started yet. */
export function isFutureMonth(competence: string, today = new Date()): boolean {
  return monthRange(competence).from > isoDay(today);
}
