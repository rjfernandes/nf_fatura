import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { PrismaClient } from "../generated/prisma/client.js";
import type { BillingService } from "../services/billingService.js";
import type { StatementService } from "../services/statementService.js";
import { tools } from "./tools/index.js";
import { McpDefs } from "./tools/types.js";

export function buildMcpServer(
  statements: StatementService,
  billings: BillingService,
  db: PrismaClient,
  defaultDir: string,
) {
  const server = new McpServer({ name: "nf-fatura-extrato", version: "1.0.0" });

  const mcpDefs: McpDefs = {
    server,
    statements,
    billings,
    db,
    defaultDir,
  };

  for (const tool of tools) {
    tool(mcpDefs);
  }

  return server;
}
