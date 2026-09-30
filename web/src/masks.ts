import { isCNPJ } from "validation-br";

const digits = (v: string) => v.replace(/\D/g, "");

/**
 * Alphanumeric CNPJ: 12 letters/digits (root + order) followed by 2 numeric
 * check digits.
 */
export function maskCnpj(v: string): string {
  const raw = v.replace(/[^0-9a-zA-Z]/g, "").toUpperCase();
  const d = (raw.slice(0, 12) + raw.slice(12, 14).replace(/\D/g, "")).slice(
    0,
    14,
  );
  const parts = [
    d.slice(0, 2),
    d.slice(2, 5),
    d.slice(5, 8),
    d.slice(8, 12),
    d.slice(12, 14),
  ];
  const seps = ["", ".", ".", "/", "-"];
  return parts.reduce(
    (out, part, i) => (part ? out + seps[i] + part : out),
    "",
  );
}

export function isValidCnpjInput(masked: string): boolean {
  return isCNPJ(masked.replace(/[^0-9a-zA-Z]/g, ""));
}

export function maskCep(v: string): string {
  const d = digits(v).slice(0, 8);
  return d.length > 5 ? `${d.slice(0, 5)}-${d.slice(5)}` : d;
}

/** Typing fills from the cents: 1 -> 0,01 -> 0,10 -> 1,00 -> 10,00 */
export function maskMoney(v: string): string {
  const d = digits(v).replace(/^0+/, "").slice(0, 12);
  if (!d) {
    return "";
  }
  const padded = d.padStart(3, "0");
  const int = padded.slice(0, -2).replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return `${int},${padded.slice(-2)}`;
}

export const moneyToCents = (masked: string): number =>
  Number(digits(masked) || "0");
export const centsToMasked = (cents: number): string =>
  maskMoney(String(cents));
