import { renderDanfse } from "../integrations/nfse/danfse.js";
import type { FastifyPluginAsyncZod } from "fastify-type-provider-zod";
import { z } from "zod";
import { prisma } from "../db.js";
import type { BillingService } from "../services/billingService.js";
import type { ImportService } from "../services/importService.js";
import type { DeliveryService } from "../services/deliveryService.js";

const create = z.object({
  customerIds: z.array(z.string()).min(1),
  competence: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/),
  dueDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  mode: z.enum(["AUTO", "NFSE", "SLIP", "BOTH"]).default("AUTO"),
});
const deliveryBody = z
  .object({ parts: z.enum(["NFSE", "SLIP", "BOTH"]).default("BOTH") })
  .default({ parts: "BOTH" });
const idParam = z.object({ id: z.string() });
const nfseQuery = z.object({
  number: z.string().trim().max(20).optional(),
});
const MAX_PDF_BYTES = 10 * 1024 * 1024;

export const billingRoutes =
  (
    service: BillingService,
    delivery: DeliveryService,
    importer: ImportService,
  ): FastifyPluginAsyncZod =>
  async (app) => {
    const include = {
      customer: {
        select: {
          name: true,
          nickName: true,
          cnpj: true,
          integration: true,
          station: true,
        },
      },
      invoice: { select: { number: true, accessKey: true, source: true } },
      bankSlip: { select: { linhaDigitavel: true, status: true } },
      delivery: {
        select: { status: true, parts: true, error: true, sentAt: true },
      },
    };

    // Flags which invoices and boletos have a PDF without loading the bytes of
    // each one.
    const withPdfFlag = async <
      T extends { id: string; invoice: object | null; bankSlip: object | null },
    >(
      billings: T[],
    ) => {
      const ids = billings.map((b) => b.id);
      const [invoices, slips] = await Promise.all([
        prisma.invoice.findMany({
          where: {
            billingId: { in: ids },
            OR: [
              { pdf: { not: null } },
              // Issued here: the DANFSe is rendered from the stored XML.
              { source: { not: "MANUAL" }, xml: { not: null } },
            ],
          },
          select: { billingId: true },
        }),
        prisma.bankSlip.findMany({
          where: { billingId: { in: ids }, pdf: { not: null } },
          select: { billingId: true },
        }),
      ]);
      const nfseIds = new Set(invoices.map((i) => i.billingId));
      const slipIds = new Set(slips.map((s) => s.billingId));
      return billings.map((b) => ({
        ...b,
        invoice: b.invoice && { ...b.invoice, hasPdf: nfseIds.has(b.id) },
        bankSlip: b.bankSlip && { ...b.bankSlip, hasPdf: slipIds.has(b.id) },
      }));
    };
    const findOne = async (id: string) =>
      (
        await withPdfFlag([
          await prisma.billing.findUniqueOrThrow({ where: { id }, include }),
        ])
      )[0];

    app.addContentTypeParser(
      "application/pdf",
      { parseAs: "buffer", bodyLimit: MAX_PDF_BYTES },
      (_req, body, done) => done(null, body),
    );

    app.get("/billings", async () =>
      withPdfFlag(
        await prisma.billing.findMany({
          orderBy: [{ competence: "desc" }, { createdAt: "desc" }],
          include,
          take: 200,
        }),
      ),
    );

    app.post("/billings", { schema: { body: create } }, async (req, reply) =>
      reply.code(201).send(await service.run(req.body)),
    );

    // Imports the NFS-e already issued by the company from the national portal.
    app.post("/billings/import-nfse", async () => importer.importFromNfse());

    // Imports the boletos registered at Banco Inter.
    app.post("/billings/import-slips", async () => importer.importFromSlips());

    app.post(
      "/billings/:id/retry",
      { schema: { params: idParam } },
      async (req) => {
        await service.process(req.params.id);
        return findOne(req.params.id);
      },
    );

    // Attaches the PDF of an NFS-e issued outside the system. Body is the raw
    // PDF (content-type: application/pdf).
    app.put(
      "/billings/:id/nfse.pdf",
      { schema: { params: idParam, querystring: nfseQuery } },
      async (req, reply) => {
        const pdf = req.body;
        if (
          !Buffer.isBuffer(pdf) ||
          pdf.subarray(0, 5).toString("latin1") !== "%PDF-"
        ) {
          return reply.code(400).send({ message: "Envie um arquivo PDF" });
        }
        await service.attachNfsePdf(
          req.params.id,
          pdf,
          req.query.number || undefined,
        );
        return findOne(req.params.id);
      },
    );

    // Sends the NFS-e, the boleto or both through the customer's integration.
    app.post(
      "/billings/:id/delivery",
      { schema: { params: idParam, body: deliveryBody } },
      async (req) => {
        await delivery.send(req.params.id, req.body.parts);
        return findOne(req.params.id);
      },
    );

    app.get("/integrations", async () => delivery.list());

    app.delete(
      "/billings/:id/nfse",
      { schema: { params: idParam } },
      async (req, reply) => {
        await service.removeManualNfse(req.params.id);
        return reply.code(204).send();
      },
    );

    // Only failed billings without an issued NFS-e or boleto can be discarded.
    app.delete(
      "/billings/:id",
      { schema: { params: idParam } },
      async (req, reply) => {
        const b = await prisma.billing.findUnique({
          where: { id: req.params.id },
          include: {
            invoice: { select: { id: true } },
            bankSlip: { select: { id: true } },
          },
        });
        if (!b) {
          return reply.code(404).send({ message: "Não encontrado" });
        }
        if (b.status !== "FAILED") {
          return reply
            .code(409)
            .send({ message: "Só é possível remover faturamentos com erro" });
        }
        if (b.invoice || b.bankSlip) {
          return reply.code(409).send({
            message:
              "Este faturamento já tem NFS-e ou boleto emitido; não pode ser removido",
          });
        }
        await prisma.billing.delete({ where: { id: b.id } });
        return reply.code(204).send();
      },
    );

    // Cancels the boleto at Banco Inter; the billing is removed too unless it
    // has an NFS-e.
    app.delete(
      "/billings/:id/boleto",
      { schema: { params: idParam } },
      async (req, reply) => {
        const slip = await prisma.bankSlip.findUnique({
          where: { billingId: req.params.id },
        });
        if (!slip) {
          return reply.code(404).send({ message: "Boleto não encontrado" });
        }
        return service.cancelSlip(req.params.id);
      },
    );

    app.get(
      "/billings/:id/nfse.xml",
      { schema: { params: idParam } },
      async (req, reply) => {
        const inv = await prisma.invoice.findUnique({
          where: { billingId: req.params.id },
        });
        if (!inv?.xml) {
          return reply.code(404).send({ message: "NFS-e não encontrada" });
        }
        return reply.type("application/xml").send(inv.xml);
      },
    );

    app.get(
      "/billings/:id/nfse.pdf",
      { schema: { params: idParam } },
      async (req, reply) => {
        const pdf = await service.nfsePdf(req.params.id);
        if (!pdf) {
          return reply.code(404).send({ message: "PDF não disponível" });
        }
        return reply.type("application/pdf").send(pdf);
      },
    );

    app.get(
      "/billings/:id/boleto.pdf",
      { schema: { params: idParam } },
      async (req, reply) => {
        const slip = await prisma.bankSlip.findUnique({
          where: { billingId: req.params.id },
        });
        if (!slip?.pdf) {
          return reply.code(404).send({ message: "Boleto não encontrado" });
        }
        return reply.type("application/pdf").send(Buffer.from(slip.pdf));
      },
    );
  };
