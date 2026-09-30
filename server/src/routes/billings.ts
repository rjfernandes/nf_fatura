import type { FastifyPluginAsyncZod } from "fastify-type-provider-zod";
import { z } from "zod";
import { prisma } from "../db.js";
import type { BillingService } from "../services/billingService.js";

const create = z.object({
  customerIds: z.array(z.string()).min(1),
  competence: z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/),
  dueDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  mode: z.enum(["AUTO", "NFSE", "SLIP", "BOTH"]).default("AUTO"),
});
const idParam = z.object({ id: z.string() });

export const billingRoutes =
  (service: BillingService): FastifyPluginAsyncZod =>
  async (app) => {
    const include = {
      customer: { select: { name: true, cnpj: true } },
      invoice: { select: { number: true, accessKey: true } },
      bankSlip: { select: { linhaDigitavel: true, status: true } },
    };

    app.get("/billings", async () =>
      prisma.billing.findMany({
        orderBy: { createdAt: "desc" },
        include,
        take: 200,
      }),
    );

    app.post("/billings", { schema: { body: create } }, async (req, reply) =>
      reply.code(201).send(await service.run(req.body)),
    );

    app.post(
      "/billings/:id/retry",
      { schema: { params: idParam } },
      async (req) => {
        await service.process(req.params.id);
        return prisma.billing.findUniqueOrThrow({
          where: { id: req.params.id },
          include,
        });
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
        const inv = await prisma.invoice.findUnique({
          where: { billingId: req.params.id },
        });
        if (!inv?.pdf) {
          return reply.code(404).send({ message: "PDF não disponível" });
        }
        return reply.type("application/pdf").send(Buffer.from(inv.pdf));
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
