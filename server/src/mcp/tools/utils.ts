import z from "zod";

export const competence = z
  .string()
  .regex(/^\d{4}-(0[1-9]|1[0-2])$/, "Use o formato AAAA-MM")
  .describe(
    "Mês do extrato, no formato AAAA-MM (ex.: 2026-08). Pode ser o mês atual (até hoje) ou um anterior.",
  );

export const brl = (cents: number) =>
  (cents / 100).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });

export const text = (t: string) => ({
  content: [{ type: "text" as const, text: t }],
});

export const fail = (e: unknown) => ({
  isError: true,
  ...text(e instanceof Error ? e.message : String(e)),
});
