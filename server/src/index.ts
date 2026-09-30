import { env } from "./config/env.js";
import { buildApp } from "./app.js";
import { InterApi } from "./integrations/inter/interApi.js";
import { InterSlipProvider } from "./integrations/inter/interClient.js";
import { InterStatementProvider } from "./integrations/inter/interStatement.js";
import { NfseNacionalProvider } from "./integrations/nfse/nfseNacional.js";
import { StatementService } from "./services/statementService.js";

const inter = new InterApi(env);
const account = {
  companyName: env.COMPANY_NAME,
  cnpj: env.COMPANY_CNPJ ?? "",
  branch: env.INTER_BRANCH,
  account: env.INTER_ACCOUNT ?? "",
};
const statements = new StatementService(
  new InterStatementProvider(inter),
  account,
);
const app = buildApp(
  new NfseNacionalProvider(env),
  new InterSlipProvider(inter),
  statements,
);
app.listen({ port: env.PORT, host: "0.0.0.0" });
