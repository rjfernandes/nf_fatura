import type { PrismaClient } from "../generated/prisma/client.js";
import type {
  ImportedNfse,
  ImportedSlip,
  NfseProvider,
  SlipProvider,
} from "../integrations/types.js";
import { normalizeCnpj } from "../lib/cnpj.js";
import { cityOf } from "../lib/ibge.js";

type NewCustomer = {
  name: string;
  taxId: string;
  address?: string;
  number?: string;
  complement?: string;
  neighborhood?: string;
  city?: string;
  cityIbgeCode?: string;
  state?: string;
  zipcode?: string;
  recurringValue: number;
  hasBankSlip: boolean;
};

export interface ImportResult {
  imported: number; // new billings
  linked: number; // documents attached to a billing that already existed
  duplicates: number;
  createdCustomers: number;
  skipped: { ref: string; taxId: string; name?: string }[];
}

const day = (date: Date) => date.toISOString().slice(0, 10);
const addDays = (n: number) => day(new Date(Date.now() + n * 86_400_000));

/**
 * Brings documents issued outside the system into the history: the NFS-e from
 * the national portal and the boletos from the bank. They can come in any
 * order; each one joins the billing of its customer + competence when there is
 * one (without that document yet), otherwise a billing is created.
 */
export class ImportService {
  constructor(
    private db: PrismaClient,
    private nfse: NfseProvider,
    private slip: SlipProvider,
  ) {}

  private emptyResult(): ImportResult {
    return {
      imported: 0,
      linked: 0,
      duplicates: 0,
      createdCustomers: 0,
      skipped: [],
    };
  }

  /** Customer by CNPJ, registered from the document when unknown. */
  private async customerFor(
    byCnpj: Map<string, string>,
    c: NewCustomer,
    result: ImportResult,
  ) {
    const taxId = normalizeCnpj(c.taxId);
    let id = byCnpj.get(taxId);
    // Only legal entities are customers; an individual's CPF is reported.
    if (!id && taxId.length === 14) {
      id = (
        await this.db.customer.create({
          data: {
            name: c.name || taxId,
            cnpj: taxId,
            address: c.address || "-",
            addressNumber: c.number || "S/N",
            addressComplement: c.complement,
            neighborhood: c.neighborhood || "-",
            city: c.city || "-",
            cityIbgeCode: c.cityIbgeCode,
            state: c.state || "-",
            zipcode: c.zipcode ?? "",
            recurringValue: c.recurringValue,
            hasBankSlip: c.hasBankSlip,
          },
        })
      ).id;
      byCnpj.set(taxId, id);
      result.createdCustomers++;
    }
    return id;
  }

  private async customerIds() {
    const customers = await this.db.customer.findMany({
      select: { id: true, cnpj: true },
    });
    return new Map(customers.map((c) => [normalizeCnpj(c.cnpj), c.id]));
  }

  /** The billing is done once every document it asks for is there. */
  private async settle(billingId: string) {
    const b = await this.db.billing.findUniqueOrThrow({
      where: { id: billingId },
      include: { invoice: { select: { id: true } }, bankSlip: true },
    });
    const complete =
      (!b.issueNfse || !!b.invoice) && (!b.issueSlip || !!b.bankSlip);
    if (complete) {
      await this.db.billing.update({
        where: { id: billingId },
        data: { status: "COMPLETED", error: null },
      });
    }
  }

  async importFromNfse() {
    const notes = await this.nfse.list();
    const byCnpj = await this.customerIds();
    const result = this.emptyResult();
    for (const n of notes) {
      if (
        await this.db.invoice.findUnique({ where: { accessKey: n.accessKey } })
      ) {
        result.duplicates++;
        continue;
      }
      const a = n.takerAddress;
      const known = byCnpj.has(normalizeCnpj(n.takerTaxId));
      const { city, state } = known
        ? { city: undefined, state: undefined }
        : await cityOf(a?.cityIbgeCode ?? "");
      const customerId = await this.customerFor(
        byCnpj,
        {
          name: n.takerName ?? "",
          taxId: n.takerTaxId,
          address: a?.address,
          number: a?.number,
          complement: a?.complement,
          neighborhood: a?.neighborhood,
          city,
          cityIbgeCode: a?.cityIbgeCode,
          state,
          zipcode: a?.zipcode,
          recurringValue: n.amountCents,
          hasBankSlip: false,
        },
        result,
      );
      if (!customerId) {
        result.skipped.push({
          ref: `NFS-e ${n.number}`,
          taxId: n.takerTaxId,
          name: n.takerName,
        });
        continue;
      }
      await this.attachInvoice(customerId, n, result);
    }
    return result;
  }

  private async attachInvoice(
    customerId: string,
    n: ImportedNfse,
    result: ImportResult,
  ) {
    const date = n.issuedAt.slice(0, 10);
    const competence = n.competence;
    const invoice = {
      number: n.number,
      accessKey: n.accessKey,
      xml: n.xml,
      source: "IMPORTED",
    };
    const existing = await this.db.billing.findFirst({
      where: { customerId, competence, invoice: null },
      orderBy: { createdAt: "asc" },
    });
    if (existing) {
      await this.db.billing.update({
        where: { id: existing.id },
        data: { issueNfse: true, invoice: { create: invoice } },
      });
      await this.settle(existing.id);
      result.linked++;
      return;
    }
    await this.db.billing.create({
      data: {
        customerId,
        competence,
        amountCents: n.amountCents,
        dueDate: date,
        issueNfse: true,
        issueSlip: false,
        status: "COMPLETED",
        invoice: { create: invoice },
      },
    });
    result.imported++;
  }

  /** Boletos due from ~10 months back to 2 months ahead (the bank caps a query at a year). */
  async importFromSlips(from = addDays(-300), to = addDays(60)) {
    const known = new Set(
      (
        await this.db.bankSlip.findMany({ select: { codigoSolicitacao: true } })
      ).map((s) => s.codigoSolicitacao),
    );
    const slips = await this.slip.list(from, to, known);
    const byCnpj = await this.customerIds();
    const result = this.emptyResult();
    for (const s of slips) {
      if (known.has(s.codigoSolicitacao)) {
        result.duplicates++;
        continue;
      }
      const p = s.payer;
      const customerId = await this.customerFor(
        byCnpj,
        {
          name: p.name,
          taxId: p.taxId,
          address: p.address,
          number: p.number,
          complement: p.complement,
          neighborhood: p.neighborhood,
          city: p.city,
          state: p.state,
          zipcode: p.zipcode,
          recurringValue: s.amountCents,
          hasBankSlip: true,
        },
        result,
      );
      if (!customerId) {
        result.skipped.push({
          ref: `Boleto ${s.nossoNumero ?? s.codigoSolicitacao}`,
          taxId: p.taxId,
          name: p.name,
        });
        continue;
      }
      await this.attachSlip(customerId, s, result);
    }
    return result;
  }

  private async attachSlip(
    customerId: string,
    s: ImportedSlip,
    result: ImportResult,
  ) {
    // The due date is the competence.
    const competence = s.dueDate.slice(0, 7);
    const bankSlip = {
      codigoSolicitacao: s.codigoSolicitacao,
      nossoNumero: s.nossoNumero,
      linhaDigitavel: s.linhaDigitavel,
      barcode: s.barcode,
      status: s.status,
      pdf: s.pdf ? new Uint8Array(s.pdf) : undefined,
    };
    const existing = await this.db.billing.findFirst({
      where: { customerId, competence, bankSlip: null },
      orderBy: { createdAt: "asc" },
    });
    if (existing) {
      await this.db.billing.update({
        where: { id: existing.id },
        data: {
          issueSlip: true,
          dueDate: s.dueDate,
          bankSlip: { create: bankSlip },
        },
      });
      await this.settle(existing.id);
      result.linked++;
      return;
    }
    await this.db.billing.create({
      data: {
        customerId,
        competence,
        amountCents: s.amountCents,
        dueDate: s.dueDate,
        issueNfse: false,
        issueSlip: true,
        status: "COMPLETED",
        bankSlip: { create: bankSlip },
      },
    });
    result.imported++;
  }
}
