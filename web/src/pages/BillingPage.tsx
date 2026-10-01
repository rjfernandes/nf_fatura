import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { api, brl, formatCnpj, type Billing, type BillingMode } from "../api";
import NfseUploadDialog from "../components/NfseUploadDialog";
import Table from "../components/Table";

const badge: Record<Billing["status"], string> = {
  PENDING: "bg-slate-100 text-slate-700",
  NFSE_ISSUED: "bg-amber-100 text-amber-800",
  COMPLETED: "bg-green-100 text-green-800",
  FAILED: "bg-red-100 text-red-800",
};
const label: Record<Billing["status"], string> = {
  PENDING: "Pendente",
  NFSE_ISSUED: "NFS-e emitida",
  COMPLETED: "Concluído",
  FAILED: "Falhou",
};
const modeOptions: { value: BillingMode; label: string; button: string }[] = [
  { value: "AUTO", label: "Conforme o cadastro do cliente", button: "Gerar" },
  { value: "BOTH", label: "NFS-e e boleto", button: "Gerar NFS-e e boletos" },
  { value: "NFSE", label: "Somente NFS-e", button: "Gerar NFS-e" },
  { value: "SLIP", label: "Somente boleto", button: "Gerar boletos" },
];
const input = "mt-1 rounded-md border border-slate-300 px-3 py-2 text-sm";

const now = new Date();
// Day 5 of the current month, or of the next one if day 5 has already passed.
const dueDay = new Date(
  now.getFullYear(),
  now.getMonth() + (now.getDate() > 5 ? 1 : 0),
  5,
);
const defaultDueDate = `${dueDay.getFullYear()}-${String(dueDay.getMonth() + 1).padStart(2, "0")}-05`;

// Competence is the month before the due date.
const competenceDate = new Date(dueDay.getFullYear(), dueDay.getMonth() - 1, 1);
const defaultCompetence = `${competenceDate.getFullYear()}-${String(competenceDate.getMonth() + 1).padStart(2, "0")}`;

export default function BillingPage() {
  const qc = useQueryClient();
  const customers = useQuery({
    queryKey: ["customers"],
    queryFn: api.customers,
  });
  const billings = useQuery({ queryKey: ["billings"], queryFn: api.billings });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [competence, setCompetence] = useState(defaultCompetence);
  const [dueDate, setDueDate] = useState(defaultDueDate);
  const [mode, setMode] = useState<BillingMode>("AUTO");
  const [uploadFor, setUploadFor] = useState<Billing | null>(null);

  const refresh = () => qc.invalidateQueries({ queryKey: ["billings"] });
  const generate = useMutation({
    mutationFn: () =>
      api.createBillings({
        customerIds: [...selected],
        competence,
        dueDate,
        mode,
      }),
    onSuccess: () => {
      setSelected(new Set());
      refresh();
    },
  });
  const retry = useMutation({ mutationFn: api.retry, onSuccess: refresh });
  const removeSlip = useMutation({
    mutationFn: api.deleteSlip,
    onSuccess: refresh,
    onError: (e) => alert((e as Error).message),
  });
  const remove = useMutation({
    mutationFn: api.deleteBilling,
    onSuccess: refresh,
    onError: (e) => alert((e as Error).message),
  });
  const attachNfse = useMutation({
    mutationFn: (v: { id: string; file: File; number: string }) =>
      api.attachNfse(v.id, v.file, v.number || undefined),
    onSuccess: () => {
      setUploadFor(null);
      refresh();
    },
  });
  const removeNfse = useMutation({
    mutationFn: api.deleteNfse,
    onSuccess: refresh,
    onError: (e) => alert((e as Error).message),
  });
  const openUpload = (b: Billing) => {
    attachNfse.reset();
    setUploadFor(b);
  };

  const list = customers.data ?? [];
  const allSelected = list.length > 0 && selected.size === list.length;
  const toggle = (id: string) =>
    setSelected((s) => {
      const n = new Set(s);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });
  const needsDue =
    mode === "BOTH" ||
    mode === "SLIP" ||
    (mode === "AUTO" && list.some((c) => selected.has(c.id) && c.hasBankSlip));
  const total = list
    .filter((c) => selected.has(c.id))
    .reduce((s, c) => s + c.recurringValue, 0);
  const canSubmit =
    selected.size > 0 && (!needsDue || !!dueDate) && !generate.isPending;

  return (
    <div className="space-y-8">
      <section className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm">
        <h2 className="mb-3 font-semibold">Gerar faturas</h2>
        <div className="mb-4 flex flex-wrap items-end gap-4">
          <label className="text-sm font-medium text-slate-700">
            Competência
            <input
              type="month"
              value={competence}
              onChange={(e) => setCompetence(e.target.value)}
              className={`${input} block`}
            />
          </label>
          <label className="text-sm font-medium text-slate-700">
            O que emitir
            <select
              value={mode}
              onChange={(e) => setMode(e.target.value as BillingMode)}
              className={`${input} block`}
            >
              {modeOptions.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
          <label className="text-sm font-medium text-slate-700">
            Vencimento do boleto
            <input
              type="date"
              value={dueDate}
              onChange={(e) => setDueDate(e.target.value)}
              className={`${input} block`}
            />
          </label>
        </div>
        <Table
          headers={[
            {
              label: (
                <input
                  type="checkbox"
                  checked={allSelected}
                  onChange={() =>
                    setSelected(
                      allSelected ? new Set() : new Set(list.map((c) => c.id)),
                    )
                  }
                />
              ),
              className: "w-10",
              field: (c) => (
                <input
                  type="checkbox"
                  checked={selected.has(c.id)}
                  onChange={() => toggle(c.id)}
                  onClick={(e) => e.stopPropagation()}
                />
              ),
            },
            {
              label: "Cliente",
              field: (c) => <span className="font-medium">{c.name}</span>,
            },
            { label: "CNPJ", field: (c) => formatCnpj(c.cnpj) },
            {
              label: "Valor",
              className: "text-right",
              field: (c) => brl(c.recurringValue),
            },
            { label: "Boleto", field: (c) => (c.hasBankSlip ? "Sim" : "Não") },
          ]}
          rows={list}
          rowKey={(c) => c.id}
          onRowClick={(c) => toggle(c.id)}
          emptyMessage="Cadastre clientes primeiro."
          cellPadding="p-2"
          headClassName=""
        />
        <div className="mt-4 flex items-center gap-4">
          <button
            disabled={!canSubmit}
            onClick={() => generate.mutate()}
            className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
          >
            {generate.isPending
              ? "Gerando..."
              : `${modeOptions.find((o) => o.value === mode)!.button} (${selected.size})`}
          </button>
          <span className="text-sm text-slate-600">Total: {brl(total)}</span>
          {needsDue && !dueDate && (
            <span className="text-sm text-amber-700">
              Informe o vencimento para os boletos.
            </span>
          )}
        </div>
        {generate.error && (
          <p className="mt-2 text-sm text-red-600">
            {(generate.error as Error).message}
          </p>
        )}
      </section>

      <section>
        <h2 className="mb-3 font-semibold">Histórico</h2>
        <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white shadow-sm">
          <Table
            headers={[
              {
                label: "Cliente",
                field: (b) => (
                  <span className="font-medium">{b.customer.name}</span>
                ),
              },
              { label: "Comp.", field: "competence" },
              {
                label: "Valor",
                className: "text-right",
                field: (b) => brl(b.amountCents),
              },
              {
                label: "Status",
                field: (b) => (
                  <>
                    <span
                      className={`rounded-full px-2 py-0.5 text-xs font-medium ${badge[b.status]}`}
                    >
                      {label[b.status]}
                    </span>
                    {b.error && (
                      <p className="mt-1 max-w-xs wrap-break-words text-xs text-red-600">
                        {b.error}
                      </p>
                    )}
                  </>
                ),
              },
              {
                label: "NFS-e",
                className: "space-x-2",
                field: (b) =>
                  b.invoice ? (
                    <>
                      <span>{b.invoice.number ?? "—"}</span>
                      {b.invoice.source === "MANUAL" && (
                        <span className="rounded-full bg-slate-100 px-2 py-0.5 text-xs text-slate-600">
                          manual
                        </span>
                      )}
                      {b.invoice.hasPdf ? (
                        <a
                          className="text-indigo-600 hover:underline"
                          href={`/api/billings/${b.id}/nfse.pdf`}
                          target="_blank"
                        >
                          PDF
                        </a>
                      ) : (
                        <button
                          onClick={() => openUpload(b)}
                          className="text-indigo-600 hover:underline"
                        >
                          Anexar PDF
                        </button>
                      )}
                      {b.invoice.source === "API" && (
                        <a
                          className="text-indigo-600 hover:underline"
                          href={`/api/billings/${b.id}/nfse.xml`}
                          target="_blank"
                        >
                          XML
                        </a>
                      )}
                      {b.invoice.source === "MANUAL" && (
                        <>
                          <button
                            onClick={() => openUpload(b)}
                            className="text-indigo-600 hover:underline"
                          >
                            Substituir
                          </button>
                          <button
                            onClick={() =>
                              confirm(
                                `Remover a NFS-e anexada de ${b.customer.name} (${b.competence})?`,
                              ) && removeNfse.mutate(b.id)
                            }
                            disabled={removeNfse.isPending}
                            className="text-red-600 hover:underline disabled:opacity-50"
                          >
                            Remover
                          </button>
                        </>
                      )}
                    </>
                  ) : (
                    <button
                      onClick={() => openUpload(b)}
                      className="text-indigo-600 hover:underline"
                    >
                      Anexar
                    </button>
                  ),
              },
              {
                label: "Boleto",
                className: "space-x-2",
                field: (b) =>
                  b.bankSlip ? (
                    <>
                      <a
                        className="text-indigo-600 hover:underline"
                        href={`/api/billings/${b.id}/boleto.pdf`}
                        target="_blank"
                      >
                        PDF
                      </a>
                      <button
                        onClick={() =>
                          confirm(
                            `Cancelar o boleto de ${b.customer.name} (${b.competence}) no Banco Inter? Essa ação não pode ser desfeita.` +
                              (b.invoice
                                ? " A NFS-e emitida será mantida no histórico."
                                : " O faturamento também será removido do histórico."),
                          ) && removeSlip.mutate(b.id)
                        }
                        disabled={removeSlip.isPending}
                        className="text-red-600 hover:underline disabled:opacity-50"
                      >
                        Excluir
                      </button>
                    </>
                  ) : (
                    "—"
                  ),
              },
              {
                label: "",
                className: "space-x-3 text-right",
                field: (b) =>
                  b.status === "FAILED" && (
                    <>
                      <button
                        onClick={() => retry.mutate(b.id)}
                        disabled={retry.isPending}
                        className="text-indigo-600 hover:underline disabled:opacity-50"
                      >
                        Tentar novamente
                      </button>
                      {!b.invoice && !b.bankSlip && (
                        <button
                          onClick={() =>
                            confirm(
                              `Remover o faturamento de ${b.customer.name} (${b.competence})?`,
                            ) && remove.mutate(b.id)
                          }
                          disabled={remove.isPending}
                          className="text-red-600 hover:underline disabled:opacity-50"
                        >
                          Remover
                        </button>
                      )}
                    </>
                  ),
              },
            ]}
            rows={billings.data ?? []}
            rowKey={(b) => b.id}
            rowClassName="align-top"
            emptyMessage="Nenhum faturamento ainda."
          />
        </div>
        <NfseUploadDialog
          billing={uploadFor}
          pending={attachNfse.isPending}
          error={attachNfse.error}
          onSubmit={(file, number) =>
            uploadFor && attachNfse.mutate({ id: uploadFor.id, file, number })
          }
          onClose={() => setUploadFor(null)}
        />
      </section>
    </div>
  );
}
