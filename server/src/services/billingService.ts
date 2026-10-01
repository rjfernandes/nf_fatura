import type { PrismaClient } from "../generated/prisma/client.js";
import { renderDanfse } from "../integrations/nfse/danfse.js";
import type { NfseProvider, SlipProvider } from "../integrations/types.js";
import { httpError } from "../lib/httpError.js";

/**
 * AUTO follows each customer's registration: NFS-e always, boleto only if
 * hasBankSlip.
 */
export type BillingMode = "AUTO" | "NFSE" | "SLIP" | "BOTH";

export interface BillingInput {
  customerIds: string[];
  competence: string;
  dueDate: string;
  mode?: BillingMode;
}

function resolveMode(mode: BillingMode, hasBankSlip: boolean) {
  switch (mode) {
    case "NFSE":
      return { issueNfse: true, issueSlip: false };
    case "SLIP":
      return { issueNfse: false, issueSlip: true };
    case "BOTH":
      return { issueNfse: true, issueSlip: true };
    default:
      return { issueNfse: true, issueSlip: hasBankSlip };
  }
}

const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));

export class BillingService {
  constructor(
    private db: PrismaClient,
    private nfse: NfseProvider,
    private slip: SlipProvider,
  ) {}

  /** Creates one Billing per customer and processes each independently. */
  async run(input: BillingInput) {
    const customers = await this.db.customer.findMany({
      where: { id: { in: input.customerIds } },
    });
    const billings = [];
    for (const c of customers) {
      const b = await this.db.billing.create({
        data: {
          customerId: c.id,
          competence: input.competence,
          amountCents: c.recurringValue,
          dueDate: input.dueDate,
          ...resolveMode(input.mode ?? "AUTO", c.hasBankSlip),
        },
      });
      billings.push(b.id);
    }
    // Sequential to respect API rate limits and keep ordering predictable.
    for (const id of billings) {
      await this.process(id);
    }
    return this.db.billing.findMany({
      where: { id: { in: billings } },
      include: {
        customer: true,
        invoice: { select: { number: true, accessKey: true } },
        bankSlip: { select: { linhaDigitavel: true, status: true } },
      },
    });
  }

  /**
   * Cancels the boleto at the bank and then drops it locally. The bank goes
   * first: if it refuses (already paid, for instance) nothing changes here.
   * Without an NFS-e the billing has nothing left, so it leaves the history
   * too; with one, the billing stays (an issued NFS-e must not vanish) and
   * issueSlip is cleared so a retry won't recreate the boleto.
   */
  async cancelSlip(billingId: string): Promise<{ billingRemoved: boolean }> {
    const b = await this.db.billing.findUniqueOrThrow({
      where: { id: billingId },
      include: {
        bankSlip: true,
        invoice: { select: { id: true } },
        delivery: { select: { status: true } },
      },
    });
    if (!b.bankSlip) {
      throw new Error("Boleto não encontrado");
    }
    if (b.delivery?.status === "SENT") {
      throw httpError(409, "Boleto já enviado ao cliente");
    }
    await this.slip.cancel(
      b.bankSlip.codigoSolicitacao,
      "Cancelado pelo emissor",
    );
    if (b.invoice) {
      await this.db.$transaction([
        this.db.bankSlip.delete({ where: { billingId } }),
        this.db.billing.update({
          where: { id: billingId },
          data: { issueSlip: false },
        }),
      ]);
      return { billingRemoved: false };
    }
    await this.db.$transaction([
      this.db.bankSlip.delete({ where: { billingId } }),
      this.db.delivery.deleteMany({ where: { billingId } }),
      this.db.billing.delete({ where: { id: billingId } }),
    ]);
    return { billingRemoved: true };
  }

  /**
   * Attaches the PDF of an NFS-e issued outside the system (Emissor Web). It
   * becomes a MANUAL invoice, so process() won't issue another one through the
   * API. An API invoice only gets the PDF when it has none (DANFSe download
   * failed); an official PDF is never overwritten.
   */
  async attachNfsePdf(billingId: string, pdf: Buffer, number?: string) {
    const b = await this.db.billing.findUnique({
      where: { id: billingId },
      include: {
        invoice: { select: { id: true, source: true, pdf: true } },
        delivery: { select: { status: true } },
      },
    });
    if (!b) {
      throw httpError(404, "Faturamento não encontrado");
    }
    if (b.delivery?.status === "SENT") {
      throw httpError(409, "NFS-e já enviada ao cliente");
    }
    const data = new Uint8Array(pdf);
    if (!b.invoice) {
      await this.db.$transaction([
        this.db.invoice.create({
          data: { billingId, source: "MANUAL", pdf: data, number },
        }),
        this.db.billing.update({
          where: { id: billingId },
          data: { issueNfse: true },
        }),
      ]);
      return;
    }
    if (b.invoice.source === "MANUAL") {
      await this.db.invoice.update({
        where: { billingId },
        data: { pdf: data, ...(number ? { number } : {}) },
      });
      return;
    }
    if (b.invoice.pdf) {
      throw httpError(409, "Esta NFS-e já tem o PDF oficial");
    }
    await this.db.invoice.update({ where: { billingId }, data: { pdf: data } });
  }

  /**
   * Removes a MANUAL invoice. With a boleto, issueNfse is cleared so a retry
   * won't issue a new NFS-e through the API.
   */
  async removeManualNfse(billingId: string) {
    const inv = await this.db.invoice.findUnique({
      where: { billingId },
      select: {
        source: true,
        billing: {
          select: {
            bankSlip: { select: { id: true } },
            delivery: { select: { status: true } },
          },
        },
      },
    });
    if (!inv) {
      throw httpError(404, "NFS-e não encontrada");
    }
    if (inv.billing.delivery?.status === "SENT") {
      throw httpError(409, "NFS-e já enviada ao cliente");
    }
    if (inv.source !== "MANUAL") {
      throw httpError(409, "Só é possível remover NFS-e anexada manualmente");
    }
    await this.db.$transaction([
      this.db.invoice.delete({ where: { billingId } }),
      ...(inv.billing.bankSlip
        ? [
            this.db.billing.update({
              where: { id: billingId },
              data: { issueNfse: false },
            }),
          ]
        : []),
    ]);
  }

  /**
   * Makes sure the competence has the requested documents: reuses the
   * competence's billing when there is one, otherwise creates it. Documents
   * that already exist are not issued again.
   */
  async issueDocuments(input: {
    customerId: string;
    competence: string;
    dueDate: string;
    nfse: boolean;
    slip: boolean;
  }) {
    const existing = await this.db.billing.findFirst({
      where: { customerId: input.customerId, competence: input.competence },
      include: { bankSlip: { select: { id: true } } },
    });
    let id: string;
    if (existing) {
      id = existing.id;
      await this.db.billing.update({
        where: { id },
        data: {
          issueNfse: existing.issueNfse || input.nfse,
          issueSlip: existing.issueSlip || input.slip,
          ...(input.slip && !existing.bankSlip && { dueDate: input.dueDate }),
        },
      });
    } else {
      const c = await this.db.customer.findUniqueOrThrow({
        where: { id: input.customerId },
      });
      id = (
        await this.db.billing.create({
          data: {
            customerId: c.id,
            competence: input.competence,
            amountCents: c.recurringValue,
            dueDate: input.dueDate,
            issueNfse: input.nfse,
            issueSlip: input.slip,
          },
        })
      ).id;
    }
    await this.process(id);
    return this.db.billing.findUniqueOrThrow({
      where: { id },
      include: { invoice: { select: { id: true } }, bankSlip: true },
    });
  }

  /** PDF of the NFS-e: rendered from the XML when issued here, else stored. */
  async nfsePdf(billingId: string): Promise<Buffer | null> {
    const inv = await this.db.invoice.findUnique({
      where: { billingId },
      include: { billing: { select: { customer: true } } },
    });
    if (inv?.source === "API" && inv.xml) {
      const { city, state } = inv.billing.customer;
      return renderDanfse(inv.xml, {
        customerCity: city,
        customerState: state,
      });
    }
    return inv?.pdf ? Buffer.from(inv.pdf) : null;
  }

  /** Resumes from the failed step; safe to call on FAILED billings. */
  async process(billingId: string) {
    const b = await this.db.billing.findUniqueOrThrow({
      where: { id: billingId },
      include: { customer: true, invoice: true, bankSlip: true },
    });
    try {
      if (b.issueNfse && !b.invoice) {
        const seq = await this.db.dpsSequence.upsert({
          where: { billingId: b.id },
          create: { billingId: b.id },
          update: {},
        });
        const r = await this.nfse.issue({
          billingId: b.id,
          dpsNumber: seq.id,
          customer: b.customer,
          amountCents: b.amountCents,
          competence: b.competence,
        });
        await this.db.invoice.create({
          data: {
            billingId: b.id,
            number: r.number,
            accessKey: r.accessKey,
            xml: r.xml,
            pdf: r.pdf ? new Uint8Array(r.pdf) : undefined,
            raw: r.raw,
          },
        });
        await this.db.billing.update({
          where: { id: b.id },
          data: { status: "NFSE_ISSUED", error: null },
        });
      }
      if (b.issueSlip && !b.bankSlip) {
        const s = await this.slip.create({
          billingId: b.id,
          customer: b.customer,
          amountCents: b.amountCents,
          dueDate: b.dueDate,
        });
        await this.db.bankSlip.create({
          data: {
            billingId: b.id,
            codigoSolicitacao: s.codigoSolicitacao,
            nossoNumero: s.nossoNumero,
            linhaDigitavel: s.linhaDigitavel,
            barcode: s.barcode,
            pdf: s.pdf ? new Uint8Array(s.pdf) : undefined,
            status: s.status,
          },
        });
      }
      await this.db.billing.update({
        where: { id: b.id },
        data: { status: "COMPLETED", error: null },
      });
    } catch (e) {
      await this.db.billing.update({
        where: { id: b.id },
        data: { status: "FAILED", error: msg(e) },
      });
    }
  }
}
