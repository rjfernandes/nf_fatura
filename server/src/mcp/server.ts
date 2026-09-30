import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { StatementService } from "../services/statementService.js";
import { tools } from "./tools/index.js";
import { McpDefs } from "./tools/types.js";

export function buildMcpServer(
  statements: StatementService,
  defaultDir: string,
) {
  const server = new McpServer({ name: "nf-fatura-extrato", version: "1.0.0" });

  const mcpDefs: McpDefs = {
    server,
    statements,
    defaultDir,
  };

  for (const tool of tools) {
    tool(mcpDefs);
  }

  return server;
}
