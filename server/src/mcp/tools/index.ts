import { exportStatement } from "./export_statement.js";
import { getBillingDocuments } from "./get_billing_documents.js";
import { issueBillingDocuments } from "./issue_billing_documents.js";
import { getStatement } from "./get_statement.js";
import { ToolDef } from "./types.js";

export const tools: ToolDef[] = [
  exportStatement,
  getStatement,
  getBillingDocuments,
  issueBillingDocuments,
];
