export interface Customer {
  id: string;
  name: string;
  cnpj: string;
  address: string;
  addressNumber: string;
  addressComplement: string | null;
  neighborhood: string;
  city: string;
  cityIbgeCode: string | null;
  state: string;
  zipcode: string;
  nickName: string | null;
  station: string | null; // posto, one uppercase letter
  recurringValue: number; // cents
  hasBankSlip: boolean;
  integration: IntegrationKey | null;
}
export type CustomerInput = Omit<Customer, "id">;

export type IntegrationKey = "CAMIM";

/** Customer fields an integration may require, with their display names. */
export const DELIVERY_FIELDS = { station: "posto" } as const;
export type DeliveryField = keyof typeof DELIVERY_FIELDS;

export interface Integration {
  key: IntegrationKey;
  label: string;
  configured: boolean;
  requiredFields: DeliveryField[];
}

/** Why the customer's integration is incomplete, or null when it is not. */
export function incompleteIntegration(
  integration: Integration | undefined,
  customer: Partial<Record<DeliveryField, string | null>>,
) {
  const missing = integration?.requiredFields.filter((f) => !customer[f]) ?? [];
  return missing.length
    ? `Integração ${integration!.label} incompleta: cliente sem ${missing
        .map((f) => DELIVERY_FIELDS[f])
        .join(", ")}`
    : null;
}

export type DeliveryParts = "NFSE" | "SLIP" | "BOTH";

export type BillingMode = "AUTO" | "NFSE" | "SLIP" | "BOTH";

export interface Billing {
  id: string;
  competence: string;
  amountCents: number;
  dueDate: string;
  issueNfse: boolean;
  issueSlip: boolean;
  status: "PENDING" | "NFSE_ISSUED" | "COMPLETED" | "FAILED";
  error: string | null;
  createdAt: string;
  customer: {
    name: string;
    nickName: string | null;
    cnpj: string;
    integration: IntegrationKey | null;
    station: string | null;
  };
  invoice: {
    number: string | null;
    accessKey: string | null;
    source: "API" | "MANUAL" | "IMPORTED";
    hasPdf: boolean;
  } | null;
  bankSlip: {
    linhaDigitavel: string | null;
    status: string | null;
    hasPdf: boolean;
  } | null;
  delivery: {
    status: "SENT" | "FAILED";
    parts: DeliveryParts;
    error: string | null;
    sentAt: string | null;
  } | null;
}

export interface StatementEntry {
  id: string;
  date: string;
  type: string;
  operation: "C" | "D";
  amountCents: number;
  title: string;
  description: string;
}

export interface Statement {
  competence: string;
  from: string;
  to: string;
  entries: StatementEntry[];
  totals: { credits: number; debits: number; net: number };
}

async function call<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  // Files go as-is with their own content type; anything else as JSON.
  const res = await fetch(`/api${path}`, {
    method,
    headers:
      body instanceof Blob
        ? { "content-type": body.type }
        : body
          ? { "content-type": "application/json" }
          : undefined,
    body: body instanceof Blob ? body : body ? JSON.stringify(body) : undefined,
  });
  if (res.status === 204) {
    return undefined as T;
  }
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    const details = Array.isArray(data.details)
      ? ": " +
        data.details
          .map((d: any) => `${d.instancePath?.slice(1)} ${d.message}`)
          .join("; ")
      : "";
    throw new Error((data.message ?? `Erro ${res.status}`) + details);
  }
  return data as T;
}

export const api = {
  customers: () => call<Customer[]>("GET", "/customers"),
  createCustomer: (c: CustomerInput) => call<Customer>("POST", "/customers", c),
  updateCustomer: (id: string, c: CustomerInput) =>
    call<Customer>("PUT", `/customers/${id}`, c),
  deleteCustomer: (id: string) => call<void>("DELETE", `/customers/${id}`),
  billings: () => call<Billing[]>("GET", "/billings"),
  createBillings: (b: {
    customerIds: string[];
    competence: string;
    dueDate: string;
    mode: BillingMode;
  }) => call<Billing[]>("POST", "/billings", b),
  statement: (competence: string) =>
    call<Statement>("GET", `/statements?competence=${competence}`),
  deleteSlip: (id: string) =>
    call<{ billingRemoved: boolean }>("DELETE", `/billings/${id}/boleto`),
  deleteBilling: (id: string) => call<void>("DELETE", `/billings/${id}`),
  importDocuments: (from: "nfse" | "slips") =>
    call<{
      imported: number;
      linked: number;
      duplicates: number;
      createdCustomers: number;
      skipped: { ref: string; taxId: string; name?: string }[];
    }>("POST", `/billings/import-${from}`),
  retry: (id: string) => call<Billing>("POST", `/billings/${id}/retry`),
  attachNfse: (id: string, file: File, number?: string) =>
    call<Billing>(
      "PUT",
      `/billings/${id}/nfse.pdf` +
        (number ? `?number=${encodeURIComponent(number)}` : ""),
      new Blob([file], { type: "application/pdf" }),
    ),
  deleteNfse: (id: string) => call<void>("DELETE", `/billings/${id}/nfse`),
  integrations: () => call<Integration[]>("GET", "/integrations"),
  sendDelivery: (v: { id: string; parts: DeliveryParts }) =>
    call<Billing>("POST", `/billings/${v.id}/delivery`, { parts: v.parts }),
};

export const brl = (cents: number) =>
  (cents / 100).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });
export const formatCnpj = (v: string) =>
  v.replace(/^(.{2})(.{3})(.{3})(.{4})(.{2})$/, "$1.$2.$3/$4-$5");
