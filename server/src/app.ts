import cors from "@fastify/cors";
import Fastify from "fastify";
import {
  serializerCompiler,
  validatorCompiler,
} from "fastify-type-provider-zod";
import { ZodError } from "zod";
import { prisma } from "./db.js";
import { billingRoutes } from "./routes/billings.js";
import { customerRoutes } from "./routes/customers.js";
import { BillingService } from "./services/billingService.js";
import { statementRoutes } from "./routes/statements.js";
import type { StatementService } from "./services/statementService.js";
import type { NfseProvider, SlipProvider } from "./integrations/types.js";

export function buildApp(
  nfse: NfseProvider,
  slip: SlipProvider,
  statements: StatementService,
) {
  const app = Fastify({ logger: true });
  app.setValidatorCompiler(validatorCompiler);
  app.setSerializerCompiler(serializerCompiler);
  app.register(cors, {
    origin: true,
    methods: ["GET", "POST", "PUT", "DELETE"],
  });
  app.setErrorHandler((error, _req, reply) => {
    const err = error as Error & { statusCode?: number; validation?: unknown };
    if (error instanceof ZodError) {
      return reply
        .code(400)
        .send({ message: "Dados inválidos", details: error.message });
    }
    if (err.validation) {
      return reply.code(400).send({
        message: "Dados inválidos",
        details: err.validation ?? err.message,
      });
    }
    app.log.error(err);
    return reply.code(err.statusCode ?? 500).send({ message: err.message });
  });
  app.register(customerRoutes);
  app.register(billingRoutes(new BillingService(prisma, nfse, slip)));
  app.register(statementRoutes(statements));
  return app;
}
