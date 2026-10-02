export interface BillingParty {
  name: string;
  cnpj: string;
  address: string;
  addressNumber: string;
  addressComplement?: string | null;
  neighborhood: string;
  city: string;
  cityIbgeCode?: string | null;
  state: string;
  zipcode: string;
}

export interface NfseRequest {
  billingId: string;
  dpsNumber: number; // sequential nDPS, stable across retries
  customer: BillingParty;
  amountCents: number;
  competence: string; // YYYY-MM
}

export interface NfseResult {
  number?: string;
  accessKey?: string;
  xml?: string;
  pdf?: Buffer;
  raw?: string;
}

/** An NFS-e already issued by the company, read back from the portal. */
export interface ImportedNfse {
  accessKey: string;
  number: string;
  xml: string;
  issuedAt: string; // dhEmi as written in the XML, e.g. 2026-09-30T23:50:00-03:00
  competence: string; // YYYY-MM, the note's own competence (dCompet)
  takerTaxId: string;
  takerName?: string;
  takerAddress?: {
    address?: string;
    number?: string;
    complement?: string;
    neighborhood?: string;
    cityIbgeCode?: string;
    zipcode?: string;
  };
  amountCents: number;
}

export interface NfseProvider {
  issue(req: NfseRequest): Promise<NfseResult>;
  /** NFS-e issued by the company, as the portal's DF-e distribution lists them. */
  list(): Promise<ImportedNfse[]>;
}

export interface SlipRequest {
  billingId: string;
  customer: BillingParty;
  amountCents: number;
  dueDate: string; // YYYY-MM-DD
}

export interface SlipResult {
  codigoSolicitacao: string;
  nossoNumero?: string;
  linhaDigitavel?: string;
  barcode?: string;
  pdf?: Buffer;
  status?: string;
}

/** A boleto already registered at the bank. */
export interface ImportedSlip {
  codigoSolicitacao: string;
  nossoNumero?: string;
  linhaDigitavel?: string;
  barcode?: string;
  status?: string;
  dueDate: string; // YYYY-MM-DD
  amountCents: number;
  pdf?: Buffer;
  payer: {
    taxId: string;
    name: string;
    address?: string;
    number?: string;
    complement?: string;
    neighborhood?: string;
    city?: string;
    state?: string;
    zipcode?: string;
  };
}

export interface SlipProvider {
  /**
   * Boletos due between two dates (YYYY-MM-DD), cancelled ones left out. The
   * PDF is only downloaded for codes not in `known`.
   */
  list(from: string, to: string, known: Set<string>): Promise<ImportedSlip[]>;
  create(req: SlipRequest): Promise<SlipResult>;
  /**
   * Cancels the boleto at the bank; throws if the bank refuses (e.g. already
   * paid).
   */
  cancel(codigoSolicitacao: string, reason: string): Promise<void>;
}

export interface StatementEntry {
  id: string;
  date: string; // YYYY-MM-DD
  // full timestamp when the bank provides one (orders entries within a day),
  // else the date
  at: string;
  type: string; // e.g. PIX, BOLETO_COBRANCA
  operation: "C" | "D"; // crédito | débito
  amountCents: number; // always positive; see operation
  title: string;
  description: string;
  counterpart: string; // payer/recipient name, when the bank provides it
}

export interface StatementBalance {
  available: number; // cents
  blocked: number; // cents
}

export interface StatementProvider {
  /** Entries between two dates, inclusive (YYYY-MM-DD). */
  fetch(from: string, to: string): Promise<StatementEntry[]>;
  /** Balance at the end of the given day (YYYY-MM-DD). */
  balance(date: string): Promise<StatementBalance>;
}

export interface DeliveryCustomer extends BillingParty {
  station?: string | null; // posto
}

/** Customer fields an integration may require, with their display names. */
export const DELIVERY_FIELDS = { station: "posto" } as const;
export type DeliveryField = keyof typeof DELIVERY_FIELDS;

/** What goes to the customer: the NFS-e, the boleto or both. */
export const DELIVERY_PARTS = ["NFSE", "SLIP", "BOTH"] as const;
export type DeliveryParts = (typeof DELIVERY_PARTS)[number];

export interface DeliveryRequest {
  customer: DeliveryCustomer;
  competence: string; // YYYY-MM
  nfsePdf?: Buffer; // present when the NFS-e is part of the delivery
  slipPdf?: Buffer; // present when the boleto is part of the delivery
}

/** Sends the NFS-e, the boleto or both to the customer's own system. */
export interface DeliveryProvider {
  label: string;
  configured: boolean;
  /** Customer fields that must be filled for this integration to work. */
  requiredFields: DeliveryField[];
  /**
   * Whether what is being sent for the competence is already there; detail
   * describes what was found (status, missing NFS-e...).
   */
  alreadySent(
    customer: DeliveryCustomer,
    competence: string,
    parts: DeliveryParts,
  ): Promise<{ sent: boolean; raw: string; detail?: string }>;
  send(req: DeliveryRequest): Promise<{ raw: string }>;
}
