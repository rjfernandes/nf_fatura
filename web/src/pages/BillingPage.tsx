import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import {
  api,
  brl,
  formatCnpj,
  incompleteIntegration,
  type Billing,
  type BillingMode,
  type DeliveryParts,
  type Integration,
} from "../api";
import ActionButton, { ActionLink } from "../components/ActionButton";
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
const deliveryBadge = {
  PENDING: ["bg-slate-100 text-slate-700", "Pendente"],
  SENT: ["bg-green-100 text-green-800", "Enviado"],
  FAILED: ["bg-red-100 text-red-800", "Falhou"],
} as const;

const partsOptions: { value: DeliveryParts; label: string }[] = [
  { value: "BOTH", label: "NFS-e e boleto" },
  { value: "NFSE", label: "NFS-e" },
  { value: "SLIP", label: "Boleto" },
];

// Which documents can be sent: the ones already generated, with their PDF.
const availableParts = (b: Billing) => ({
  NFSE: !!b.invoice?.hasPdf,
  SLIP: !!b.bankSlip?.hasPdf,
  BOTH: !!b.invoice?.hasPdf && !!b.bankSlip?.hasPdf,
});

// Default choice: both when available, else the one that exists.
const defaultParts = (b: Billing): DeliveryParts => {
  const a = availableParts(b);
  return a.BOTH ? "BOTH" : a.SLIP && !a.NFSE ? "SLIP" : "NFSE";
};

// Why nothing can be sent yet, if that is the case.
const missingForDelivery = (b: Billing, integration?: Integration) =>
  incompleteIntegration(integration, b.customer) ??
  (!b.invoice?.hasPdf && !b.bankSlip?.hasPdf
    ? "Falta o PDF da NFS-e e do boleto"
    : null);

// Row of action buttons that never wraps.
const actions = "flex items-center gap-1.5 whitespace-nowrap";

const formatCompetence = (c: string) => c.split("-").reverse().join("/");

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

// Competence is the current month.
const defaultCompetence = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;

// <prefix>_<competence>_<nickname, or the initials of the company name>.<ext>
const fileName = (prefix: string, ext: string, b: Billing) => {
  const { nickName, name } = b.customer;
  const who =
    nickName?.trim() ||
    name
      .split(/\s+/)
      .map((w) => w[0])
      .join("");
  return `${`${prefix}_${b.competence}_${who}`.replace(/\W+/g, "_").toLowerCase()}.${ext}`;
};

// Long errors (e.g. an HTML 502 page) are clamped, with a toggle to read all.
function ErrorText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = text.length > 140;
  return (
    <div className="mt-1 max-w-xs text-xs text-red-600">
      <p className={`wrap-break-word ${long && !open ? "line-clamp-2" : ""}`}>
        {text}
      </p>
      {long && (
        <button
          type="button"
          onClick={() => setOpen(!open)}
          className="mt-0.5 font-medium underline"
        >
          {open ? "ver menos" : "ver mais"}
        </button>
      )}
    </div>
  );
}

export default function BillingPage() {
  const qc = useQueryClient();
  const customers = useQuery({
    queryKey: ["customers"],
    queryFn: api.customers,
  });
  const billings = useQuery({ queryKey: ["billings"], queryFn: api.billings });
  const integrations = useQuery({
    queryKey: ["integrations"],
    queryFn: api.integrations,
  });
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [competence, setCompetence] = useState(defaultCompetence);
  const [dueDate, setDueDate] = useState(defaultDueDate);
  const [mode, setMode] = useState<BillingMode>("AUTO");
  const [uploadFor, setUploadFor] = useState<Billing | null>(null);

  // Returned so mutations stay pending until the history has been refetched.
  const refresh = () => qc.invalidateQueries({ queryKey: ["billings"] });
  const [progress, setProgress] = useState<{ done: number; total: number }>();
  // One request per customer, refreshing the history as each one finishes.
  const generate = useMutation({
    mutationFn: async () => {
      const ids = [...selected];
      const failures: string[] = [];
      for (const [i, customerId] of ids.entries()) {
        setProgress({ done: i, total: ids.length });
        try {
          await api.createBillings({
            customerIds: [customerId],
            competence,
            dueDate,
            mode,
          });
        } catch (e) {
          failures.push((e as Error).message);
        }
        setSelected((s) => {
          const n = new Set(s);
          n.delete(customerId);
          return n;
        });
        await refresh();
      }
      if (failures.length) {
        throw new Error(failures.join("; "));
      }
    },
    onSettled: () => setProgress(undefined),
  });
  const [retrying, setRetrying] = useState<Set<string>>(new Set());
  const markRetrying = (id: string, on: boolean) =>
    setRetrying((r) => {
      const n = new Set(r);
      on ? n.add(id) : n.delete(id);
      return n;
    });
  const retry = useMutation({
    mutationFn: async (id: string) => {
      markRetrying(id, true);
      try {
        return await api.retry(id);
      } finally {
        // Keep "Processando" until the refreshed history is in.
        await refresh();
        markRetrying(id, false);
      }
    },
    onError: (e) => alert((e as Error).message),
  });
  const importDocs = useMutation({
    mutationFn: api.importDocuments,
    onSuccess: refresh,
  });
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
  const [partsChoice, setPartsChoice] = useState<Record<string, DeliveryParts>>(
    {},
  );
  const send = useMutation({
    mutationFn: api.sendDelivery,
    onSuccess: refresh,
    onError: (e) => {
      refresh();
      alert((e as Error).message);
    },
  });
  const integrationOf = (b: Billing) =>
    integrations.data?.find((i) => i.key === b.customer.integration);
  const integrationLabel = (b: Billing) =>
    integrationOf(b)?.label ?? b.customer.integration;
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
            { label: "Apelido", field: (c) => c.nickName ?? "—" },
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
              ? `Gerando ${Math.min((progress?.done ?? 0) + 1, progress?.total ?? 1)}/${progress?.total ?? selected.size}...`
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
        <div className="mb-3 flex items-center justify-between">
          <h2 className="font-semibold">Histórico</h2>
          <div className="flex gap-2">
            {(
              [
                ["nfse", "Importar do NFSE"],
                ["slips", "Importar Boletos"],
              ] as const
            ).map(([kind, label]) => (
              <button
                key={kind}
                type="button"
                disabled={importDocs.isPending}
                onClick={() => importDocs.mutate(kind)}
                className="rounded-md border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
              >
                {importDocs.isPending && importDocs.variables === kind
                  ? "Importando..."
                  : label}
              </button>
            ))}
          </div>
        </div>
        {importDocs.error && (
          <p className="mb-2 text-sm text-red-600">
            {(importDocs.error as Error).message}
          </p>
        )}
        {importDocs.data && (
          <div className="mb-2 text-sm text-slate-600">
            <p>
              {importDocs.data.imported} importada(s), {importDocs.data.linked}{" "}
              associada(s) a faturamento existente, {importDocs.data.duplicates}{" "}
              já existente(s), {importDocs.data.createdCustomers} cliente(s)
              cadastrado(s), {importDocs.data.skipped.length} ignorada(s) (sem
              CNPJ).
            </p>
            {importDocs.data.skipped.length > 0 && (
              <ul className="list-disc pl-5 text-amber-700">
                {importDocs.data.skipped.map((s) => (
                  <li key={s.ref}>
                    {s.ref}: {s.name ?? "-"} ({formatCnpj(s.taxId)})
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
        <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white shadow-sm">
          <Table
            headers={[
              {
                label: "Cliente",
                field: (b) => (
                  <div className="w-56" title={b.customer.name}>
                    <span className="block truncate font-medium">
                      {b.customer.nickName ?? b.customer.name}
                    </span>
                    {b.customer.nickName && (
                      <span className="block truncate text-xs text-slate-500">
                        {b.customer.name}
                      </span>
                    )}
                  </div>
                ),
              },
              {
                label: "Comp. / Valor",
                className: "whitespace-nowrap",
                field: (b) => (
                  <>
                    <span className="block text-xs text-slate-500">
                      {formatCompetence(b.competence)}
                    </span>
                    <span className="font-medium">{brl(b.amountCents)}</span>
                  </>
                ),
              },
              {
                label: "Status",
                field: (b) => (
                  <>
                    {retrying.has(b.id) ? (
                      <span className="whitespace-nowrap rounded-full bg-orange-100 px-2 py-0.5 text-xs font-medium text-orange-800">
                        Processando
                      </span>
                    ) : (
                      <>
                        <span
                          className={`whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium ${badge[b.status]}`}
                        >
                          {label[b.status]}
                        </span>
                        {b.error && <ErrorText text={b.error} />}
                      </>
                    )}
                    {b.status === "FAILED" && (
                      <div className={`${actions} mt-2`}>
                        <ActionButton onClick={() => retry.mutateAsync(b.id)}>
                          Tentar novamente
                        </ActionButton>
                        {!b.invoice && !b.bankSlip && (
                          <ActionButton
                            variant="danger"
                            onClick={() =>
                              confirm(
                                `Remover o faturamento de ${b.customer.name} (${b.competence})?`,
                              ) && remove.mutateAsync(b.id)
                            }
                          >
                            Remover
                          </ActionButton>
                        )}
                      </div>
                    )}
                  </>
                ),
              },
              {
                label: "NFS-e",
                field: (b) =>
                  b.invoice ? (
                    <>
                      <div className="mb-1.5 flex items-center gap-2 whitespace-nowrap">
                        <span className="font-medium">
                          {b.invoice.number ? `Nº ${b.invoice.number}` : "—"}
                        </span>
                        {b.invoice.source === "MANUAL" && (
                          <span className="rounded-full bg-slate-100 px-2 py-0.5 text-xs text-slate-600">
                            manual
                          </span>
                        )}
                      </div>
                      <div className={actions}>
                        {b.invoice.hasPdf ? (
                          <>
                            <ActionLink href={`/api/billings/${b.id}/nfse.pdf`}>
                              PDF
                            </ActionLink>
                            <ActionLink
                              href={`/api/billings/${b.id}/nfse.pdf`}
                              download={fileName("nota_fiscal", "pdf", b)}
                            >
                              Baixar
                            </ActionLink>
                          </>
                        ) : (
                          <ActionButton onClick={() => openUpload(b)}>
                            Anexar PDF
                          </ActionButton>
                        )}
                        {b.invoice.source !== "MANUAL" && (
                          <ActionLink
                            href={`/api/billings/${b.id}/nfse.xml`}
                            download={fileName("nota_fiscal", "xml", b)}
                          >
                            XML
                          </ActionLink>
                        )}
                        {b.invoice.source === "MANUAL" &&
                          b.delivery?.status !== "SENT" && (
                            <>
                              <ActionButton onClick={() => openUpload(b)}>
                                Substituir
                              </ActionButton>
                              <ActionButton
                                variant="danger"
                                onClick={() =>
                                  confirm(
                                    `Remover a NFS-e anexada de ${b.customer.name} (${b.competence})?`,
                                  ) && removeNfse.mutateAsync(b.id)
                                }
                              >
                                Remover
                              </ActionButton>
                            </>
                          )}
                      </div>
                    </>
                  ) : (
                    <ActionButton onClick={() => openUpload(b)}>
                      Anexar
                    </ActionButton>
                  ),
              },
              {
                label: "Boleto",
                field: (b) =>
                  b.bankSlip ? (
                    <>
                      <div className="mb-1.5 whitespace-nowrap font-medium">
                        Venc. {b.dueDate.split("-").reverse().join("/")}
                      </div>
                      <div className={actions}>
                        <ActionLink href={`/api/billings/${b.id}/boleto.pdf`}>
                          PDF
                        </ActionLink>
                        <ActionLink
                          href={`/api/billings/${b.id}/boleto.pdf`}
                          download={fileName("boleto", "pdf", b)}
                        >
                          Baixar
                        </ActionLink>
                        {b.delivery?.status !== "SENT" && (
                          <ActionButton
                            variant="danger"
                            onClick={() =>
                              confirm(
                                `Cancelar o boleto de ${b.customer.name} (${b.competence}) no Banco Inter? Essa ação não pode ser desfeita.` +
                                  (b.invoice
                                    ? " A NFS-e emitida será mantida no histórico."
                                    : " O faturamento também será removido do histórico."),
                              ) && removeSlip.mutateAsync(b.id)
                            }
                          >
                            Excluir
                          </ActionButton>
                        )}
                      </div>
                    </>
                  ) : (
                    <span className="text-slate-400">—</span>
                  ),
              },
              {
                label: "Envio",
                field: (b) => {
                  if (!b.customer.integration) {
                    return <span className="text-slate-400">—</span>;
                  }
                  const status = b.delivery?.status ?? "PENDING";
                  const incomplete = incompleteIntegration(
                    integrationOf(b),
                    b.customer,
                  );
                  const missing = missingForDelivery(b, integrationOf(b));
                  const available = availableParts(b);
                  // Falls back to the default if the chosen one is gone.
                  const chosen =
                    partsChoice[b.id] && available[partsChoice[b.id]]
                      ? partsChoice[b.id]
                      : defaultParts(b);
                  const chosenLabel = partsOptions.find(
                    (o) => o.value === chosen,
                  )!.label;
                  return (
                    <>
                      <div className="mb-1.5">
                        <select
                          className="rounded-md border border-slate-300 px-1.5 py-1 text-xs disabled:opacity-50"
                          value={chosen}
                          disabled={status === "SENT" || !!missing}
                          onChange={(e) =>
                            setPartsChoice((c) => ({
                              ...c,
                              [b.id]: e.target.value as DeliveryParts,
                            }))
                          }
                        >
                          {partsOptions.map((o) => (
                            <option
                              key={o.value}
                              value={o.value}
                              disabled={!available[o.value]}
                            >
                              {o.label}
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className={actions}>
                        <ActionButton
                          disabled={status === "SENT" || !!missing}
                          title={
                            status === "SENT"
                              ? "Já enviado"
                              : (missing ??
                                `Enviar para ${integrationLabel(b)}`)
                          }
                          onClick={() =>
                            confirm(
                              `Enviar ${chosenLabel} de ${b.customer.name} (${b.competence}) para ${integrationLabel(b)}?`,
                            ) && send.mutateAsync({ id: b.id, parts: chosen })
                          }
                        >
                          Enviar
                        </ActionButton>
                        <span
                          className={`whitespace-nowrap rounded-full px-2 py-0.5 text-xs font-medium ${deliveryBadge[status][0]}`}
                          title={
                            b.delivery?.sentAt
                              ? new Date(b.delivery.sentAt).toLocaleString(
                                  "pt-BR",
                                )
                              : undefined
                          }
                        >
                          {deliveryBadge[status][1]}
                        </span>
                      </div>
                      {incomplete && status !== "SENT" && (
                        <p className="mt-1 max-w-xs text-xs text-amber-700">
                          {incomplete}
                        </p>
                      )}
                      {b.delivery?.error &&
                        (status === "FAILED" ? (
                          <ErrorText text={b.delivery.error} />
                        ) : (
                          <p className="mt-1 max-w-xs text-xs text-slate-500">
                            {b.delivery.error}
                          </p>
                        ))}
                    </>
                  );
                },
              },
            ]}
            rows={billings.data ?? []}
            rowKey={(b) => b.id}
            rowClassName="align-top"
            cellPadding="px-3 py-2.5"
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
