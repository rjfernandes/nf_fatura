import { McpServer } from "@modelcontextprotocol/sdk/server/mcp";
import { StatementService } from "../../services/statementService.js";

export type McpDefs = {
  server: McpServer;
  statements: StatementService;
  defaultDir: string;
};

export type ToolDef = (defs: McpDefs) => void;
