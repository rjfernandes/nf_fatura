// stdio MCP server. Run from anywhere: it loads server/.env and resolves
// ./certs relative to server/.
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { config } from "dotenv";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
process.chdir(root);
// stdout belongs to the MCP protocol
config({ path: resolve(root, ".env"), quiet: true });

const [
  { StdioServerTransport },
  { env },
  { InterApi },
  { InterStatementProvider },
  { StatementService },
  { BillingService },
  { NfseNacionalProvider },
  { InterSlipProvider },
  { prisma },
  { buildMcpServer },
] = await Promise.all([
  import("@modelcontextprotocol/sdk/server/stdio.js"),
  import("../config/env.js"),
  import("../integrations/inter/interApi.js"),
  import("../integrations/inter/interStatement.js"),
  import("../services/statementService.js"),
  import("../services/billingService.js"),
  import("../integrations/nfse/nfseNacional.js"),
  import("../integrations/inter/interClient.js"),
  import("../db.js"),
  import("./server.js"),
]);

const inter = new InterApi(env);
const statements = new StatementService(new InterStatementProvider(inter), {
  companyName: env.COMPANY_NAME,
  cnpj: env.COMPANY_CNPJ ?? "",
  branch: env.INTER_BRANCH,
  account: env.INTER_ACCOUNT ?? "",
});

await buildMcpServer(
  statements,
  new BillingService(
    prisma,
    new NfseNacionalProvider(env),
    new InterSlipProvider(inter),
  ),
  prisma,
  resolve(root, "exports"),
).connect(new StdioServerTransport());
