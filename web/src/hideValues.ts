// App-wide "hide money values" switch. Hidden by default; the choice is
// remembered across visits.
import { useSyncExternalStore } from "react";
import { brl } from "./api";

const KEY = "hideValues";
const listeners = new Set<() => void>();

let hidden = (() => {
  try {
    return localStorage.getItem(KEY) !== "0";
  } catch {
    return true;
  }
})();

export function toggleHideValues() {
  hidden = !hidden;
  try {
    localStorage.setItem(KEY, hidden ? "1" : "0");
  } catch {
    // Not persisted; the toggle still works for this visit.
  }
  listeners.forEach((l) => l());
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export const useHideValues = () => useSyncExternalStore(subscribe, () => hidden);

/** `brl`, masked while values are hidden. */
export function useMoney() {
  const hide = useHideValues();
  return (cents: number) => (hide ? "R$ •••••" : brl(cents));
}
