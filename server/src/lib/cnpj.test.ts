import { describe, expect, it } from "vitest";
import { isValidCnpj, normalizeCnpj } from "./cnpj.js";

describe("isValidCnpj", () => {
  it("accepts valid numeric CNPJs, formatted or not", () => {
    expect(isValidCnpj("11.222.333/0001-81")).toBe(true);
    expect(isValidCnpj("11222333000181")).toBe(true);
  });
  it("accepts valid alphanumeric CNPJs in any case", () => {
    expect(isValidCnpj("12.ABC.345/01DE-35")).toBe(true);
    expect(isValidCnpj("12abc34501de35")).toBe(true);
  });
  it("rejects invalid ones", () => {
    expect(isValidCnpj("11222333000182")).toBe(false);
    expect(isValidCnpj("12.ABC.345/01DE-36")).toBe(false);
    expect(isValidCnpj("00000000000000")).toBe(false);
    expect(isValidCnpj("123")).toBe(false);
  });
  it("normalizes to uppercase alphanumerics", () => {
    expect(normalizeCnpj("12.abc.345/01de-35")).toBe("12ABC34501DE35");
  });
});
