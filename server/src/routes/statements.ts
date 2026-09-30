import type { FastifyPluginAsyncZod } from "fastify-type-provider-zod";
import { z } from "zod";
import {
  StatementError,
  type StatementService,
} from "../services/statementService.js";

const competenceSchema = z.string().regex(/^\d{4}-(0[1-9]|1[0-2])$/);
const query = z.object({ competence: competenceSchema });
const exportQuery = z.object({
  competence: competenceSchema,
  format: z.enum(["pdf", "ofx"]),
});

export const statementRoutes =
  (service: StatementService): FastifyPluginAsyncZod =>
  async (app) => {
    app.get(
      "/statements",
      { schema: { querystring: query } },
      async (req, reply) => {
        try {
          return await service.summary(req.query.competence);
        } catch (e) {
          if (e instanceof StatementError) {
            return reply.code(400).send({ message: e.message });
          }
          throw e;
        }
      },
    );

    app.get(
      "/statements/export",
      { schema: { querystring: exportQuery } },
      async (req, reply) => {
        try {
          const file = await service.export(
            req.query.competence,
            req.query.format,
          );
          reply.header(
            "content-disposition",
            `attachment; filename="${file.fileName}"`,
          );
          return reply.type(file.mime).send(file.content);
        } catch (e) {
          if (e instanceof StatementError) {
            return reply.code(400).send({ message: e.message });
          }
          throw e;
        }
      },
    );
  };
