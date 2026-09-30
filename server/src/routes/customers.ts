import type { FastifyPluginAsyncZod } from "fastify-type-provider-zod";
import { z } from "zod";
import { prisma } from "../db.js";
import { isValidCnpj, normalizeCnpj, onlyDigits } from "../lib/cnpj.js";

const body = z.object({
  name: z.string().min(1),
  cnpj: z
    .string()
    .refine(isValidCnpj, "CNPJ inválido")
    .transform(normalizeCnpj),
  address: z.string().min(1),
  addressNumber: z.string().min(1),
  addressComplement: z.string().nullish(),
  neighborhood: z.string().min(1),
  city: z.string().min(1),
  cityIbgeCode: z
    .string()
    .regex(/^\d{7}$/, "Código IBGE deve ter 7 dígitos")
    .nullish(),
  state: z
    .string()
    .length(2)
    .transform((s) => s.toUpperCase()),
  zipcode: z
    .string()
    .transform(onlyDigits)
    .pipe(z.string().length(8, "CEP inválido")),
  recurringValue: z.number().int().nonnegative(), // cents
  hasBankSlip: z.boolean(),
});
const idParam = z.object({ id: z.string() });

export const customerRoutes: FastifyPluginAsyncZod = async (app) => {
  app.get("/customers", async () =>
    prisma.customer.findMany({ orderBy: { name: "asc" } }),
  );

  app.post("/customers", { schema: { body } }, async (req, reply) => {
    try {
      return reply
        .code(201)
        .send(await prisma.customer.create({ data: req.body }));
    } catch (e: any) {
      if (e.code === "P2002") {
        return reply.code(409).send({ message: "CNPJ já cadastrado" });
      }
      throw e;
    }
  });

  app.put(
    "/customers/:id",
    { schema: { body, params: idParam } },
    async (req, reply) => {
      try {
        return await prisma.customer.update({
          where: { id: req.params.id },
          data: req.body,
        });
      } catch (e: any) {
        if (e.code === "P2025") {
          return reply.code(404).send({ message: "Não encontrado" });
        }
        if (e.code === "P2002") {
          return reply.code(409).send({ message: "CNPJ já cadastrado" });
        }
        throw e;
      }
    },
  );

  app.delete(
    "/customers/:id",
    { schema: { params: idParam } },
    async (req, reply) => {
      try {
        await prisma.customer.delete({ where: { id: req.params.id } });
        return reply.code(204).send();
      } catch (e: any) {
        if (e.code === "P2025") {
          return reply.code(404).send({ message: "Não encontrado" });
        }
        if (e.code === "P2003") {
          return reply
            .code(409)
            .send({ message: "Cliente possui faturamentos" });
        }
        throw e;
      }
    },
  );
};
