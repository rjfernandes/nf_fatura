import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { api, brl, type Statement, type StatementEntry } from "../api";
import Table from "../components/Table";

const input = "mt-1 rounded-md border border-slate-300 px-3 py-2 text-sm";
const outlineButton =
  "rounded-md border border-slate-300 px-4 py-2 text-sm font-medium text-slate-700 hover:bg-slate-100";

const today = new Date();
const currentCompetence = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}`;
// Default to the last closed month; the current one (partial, up to today) can
// be picked too.
const prev = new Date(today.getFullYear(), today.getMonth() - 1, 1);
const defaultCompetence = `${prev.getFullYear()}-${String(prev.getMonth() + 1).padStart(2, "0")}`;

const signed = (e: StatementEntry) =>
  e.operation === "D" ? -e.amountCents : e.amountCents;
const day = (iso: string) => iso.split("-").reverse().join("/");

function downloadCsv(s: Statement) {
  const esc = (v: string) => `"${v.replace(/"/g, '""')}"`;
  const rows = s.entries.map((e) =>
    [
      day(e.date),
      e.operation === "C" ? "Crédito" : "Débito",
      e.type,
      e.title,
      e.description,
      (signed(e) / 100).toFixed(2).replace(".", ","),
    ]
      .map(esc)
      .join(";"),
  );
  const csv = ["Data;Operação;Tipo;Título;Descrição;Valor", ...rows].join("\n");
  const url = URL.createObjectURL(
    new Blob(["﻿" + csv], { type: "text/csv;charset=utf-8" }),
  );
  const a = document.createElement("a");
  a.href = url;
  a.download = `extrato-${s.competence}.csv`;
  a.click();
  URL.revokeObjectURL(url);
}

export default function StatementPage() {
  const [competence, setCompetence] = useState(defaultCompetence);
  const fetchStatement = useMutation({ mutationFn: api.statement });
  const s = fetchStatement.data;

  return (
    <div className="space-y-6">
      <section className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm">
        <h2 className="mb-3 font-semibold">Extrato bancário (Banco Inter)</h2>
        <div className="flex flex-wrap items-end gap-4">
          <label className="text-sm font-medium text-slate-700">
            Competência
            <input
              type="month"
              value={competence}
              max={currentCompetence}
              onChange={(e) => setCompetence(e.target.value)}
              className={`${input} block`}
            />
          </label>
          <button
            disabled={!competence || fetchStatement.isPending}
            onClick={() => fetchStatement.mutate(competence)}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
          >
            {fetchStatement.isPending ? "Buscando..." : "Buscar extrato"}
          </button>
          {s &&
            (["pdf", "ofx"] as const).map((format) => (
              <a
                key={format}
                href={`/api/statements/export?competence=${s.competence}&format=${format}`}
                className={`${outlineButton} uppercase`}
              >
                Baixar {format}
              </a>
            ))}
          {s && s.entries.length > 0 && (
            <button onClick={() => downloadCsv(s)} className={outlineButton}>
              Baixar CSV
            </button>
          )}
        </div>
        {fetchStatement.error && (
          <p className="mt-3 break-words text-sm text-red-600">
            {(fetchStatement.error as Error).message}
          </p>
        )}
      </section>

      {s && (
        <section className="space-y-4">
          <div className="grid gap-3 sm:grid-cols-3">
            {[
              ["Entradas", s.totals.credits, "text-green-700"],
              ["Saídas", s.totals.debits, "text-red-700"],
              ["Saldo do mês", s.totals.net, "text-slate-900"],
            ].map(([label, value, color]) => (
              <div
                key={label as string}
                className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm"
              >
                <p className="text-xs uppercase text-slate-500">{label}</p>
                <p className={`text-xl font-semibold ${color}`}>
                  {brl(value as number)}
                </p>
              </div>
            ))}
          </div>
          <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white shadow-sm">
            <Table
              headers={[
                { label: "Data", field: (e) => day(e.date) },
                { label: "Tipo", field: "type" },
                {
                  label: "Descrição",
                  field: (e) => (
                    <>
                      <span className="font-medium">{e.title}</span>
                      {e.description && (
                        <p className="text-xs text-slate-500">
                          {e.description}
                        </p>
                      )}
                    </>
                  ),
                },
                {
                  label: "Valor",
                  className: "text-right",
                  field: (e) => (
                    <span
                      className={
                        e.operation === "D" ? "text-red-700" : "text-green-700"
                      }
                    >
                      {brl(signed(e))}
                    </span>
                  ),
                },
              ]}
              rows={s.entries}
              rowKey={(e) => e.id}
              rowClassName="align-top"
              emptyMessage="Nenhuma movimentação no período."
            />
          </div>
        </section>
      )}
    </div>
  );
}
