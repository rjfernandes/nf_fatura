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

export interface NfseProvider {
  issue(req: NfseRequest): Promise<NfseResult>;
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

export interface SlipProvider {
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
