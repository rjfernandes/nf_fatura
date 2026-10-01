import { McpServer } from "@modelcontextprotocol/sdk/server/mcp";
import type { PrismaClient } from "../../generated/prisma/client.js";
import { BillingService } from "../../services/billingService.js";
import { StatementService } from "../../services/statementService.js";

export type McpDefs = {
  server: McpServer;
  statements: StatementService;
  billings: BillingService;
  db: PrismaClient;
  defaultDir: string;
};

export type ToolDef = (defs: McpDefs) => void;
