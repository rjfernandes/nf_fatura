import { isCNPJ } from "validation-br";

export const onlyDigits = (v: string) => v.replace(/\D/g, "");

/**
 * Keeps letters and digits, uppercased: the CNPJ can be alphanumeric from 2026.
 */
export const normalizeCnpj = (v: string) =>
  v.replace(/[^0-9a-zA-Z]/g, "").toUpperCase();

export const isValidCnpj = (v: string) => isCNPJ(normalizeCnpj(v));
