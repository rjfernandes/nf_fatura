import { exportStatement } from "./export_statement.js";
import { getStatement } from "./get_statement.js";
import { ToolDef } from "./types.js";

export const tools: ToolDef[] = [exportStatement, getStatement];
