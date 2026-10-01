import type { PrismaClient } from "../generated/prisma/client.js";
import {
  INTEGRATIONS,
  type DeliveryProviders,
  type IntegrationKey,
} from "../integrations/delivery.js";
import { DELIVERY_FIELDS } from "../integrations/types.js";
import { httpError } from "../lib/httpError.js";

const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));

const isIntegration = (v: string | null): v is IntegrationKey =>
  (INTEGRATIONS as readonly string[]).includes(v ?? "");

export class DeliveryService {
  constructor(
    private db: PrismaClient,
    private providers: DeliveryProviders,
    /** PDF of the billing's NFS-e (stored or rendered from the XML). */
    private nfsePdf: (billingId: string) => Promise<Buffer | null>,
  ) {}

  /** Integrations available for the customer registration. */
  list() {
    return INTEGRATIONS.map((key) => ({
      key,
      label: this.providers[key].label,
      configured: this.providers[key].configured,
      requiredFields: this.providers[key].requiredFields,
    }));
  }

  /**
   * Sends the NFS-e + boleto pair through the customer's integration. Checks
   * the remote side first so a pair already there is not sent twice. The
   * outcome is recorded either way; a failure can be retried.
   */
  async send(billingId: string) {
    const b = await this.db.billing.findUnique({
      where: { id: billingId },
      include: {
        customer: true,
        invoice: { select: { id: true } },
        bankSlip: { select: { pdf: true } },
        delivery: { select: { status: true } },
      },
    });
    if (!b) {
      throw httpError(404, "Faturamento não encontrado");
    }
    const key = b.customer.integration;
    if (!isIntegration(key)) {
      throw httpError(409, "Cliente sem integração de envio");
    }
    const provider = this.providers[key];
    if (!provider.configured) {
      throw httpError(409, `Integração ${provider.label} não configurada`);
    }
    const missing = provider.requiredFields.filter((f) => !b.customer[f]);
    if (missing.length) {
      throw httpError(
        409,
        `Integração ${provider.label} incompleta: cliente sem ${missing
          .map((f) => DELIVERY_FIELDS[f])
          .join(", ")}`,
      );
    }
    if (b.delivery?.status === "SENT") {
      throw httpError(409, "Já enviado ao cliente");
    }
    const nfsePdf = b.invoice && (await this.nfsePdf(billingId));
    if (!nfsePdf) {
      throw httpError(409, "Falta o PDF da NFS-e");
    }
    if (!b.bankSlip?.pdf) {
      throw httpError(409, "Falta o PDF do boleto");
    }

    const record = (data: {
      status: "SENT" | "FAILED";
      error?: string | null;
      response?: string | null;
    }) => {
      const row = {
        integration: key,
        error: null,
        response: null,
        ...data,
        sentAt: data.status === "SENT" ? new Date() : null,
      };
      return this.db.delivery.upsert({
        where: { billingId },
        create: { billingId, ...row },
        update: row,
      });
    };

    try {
      const check = await provider.alreadySent(b.customer, b.competence);
      if (check.sent) {
        await record({
          status: "SENT",
          error:
            `Já constava na ${provider.label}; não reenviado` +
            (check.detail ? ` (${check.detail})` : ""),
          response: check.raw,
        });
        return;
      }
      const r = await provider.send({
        customer: b.customer,
        competence: b.competence,
        nfsePdf,
        slipPdf: Buffer.from(b.bankSlip.pdf),
      });
      await record({ status: "SENT", response: r.raw });
    } catch (e) {
      await record({ status: "FAILED", error: msg(e) });
      throw httpError(502, msg(e));
    }
  }
}
