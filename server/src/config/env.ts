import "dotenv/config";
import { z } from "zod";

const schema = z.object({
  PORT: z.coerce.number().default(3333),
  // NFS-e Nacional
  NFSE_ENV: z.enum(["homologacao", "producao"]).default("homologacao"),
  NFSE_CERT_PFX_PATH: z.string().optional(),
  NFSE_CERT_PFX_PASSWORD: z.string().optional(),
  COMPANY_CNPJ: z.string().optional(),
  COMPANY_NAME: z.string().default("MRF INFORMATICA LTDA"),
  COMPANY_MUNICIPAL_REGISTRATION: z.string().optional(),
  COMPANY_CITY_IBGE: z.string().optional(),
  NFSE_SERVICE_CODE: z.string().default("010701"), // cTribNac (6 digits)
  NFSE_MUNICIPAL_SERVICE_CODE: z.string().default("001"), // cTribMun
  NFSE_NBS_CODE: z.string().default("115013000"), // cNBS
  NFSE_SERVICE_DESCRIPTION: z
    .string()
    .default(
      "Serviços de informática prestados Valor aproximado dos tributos: 6%",
    ),
  // opSimpNac: 1 não optante, 2 MEI, 3 ME/EPP
  NFSE_SIMPLES_NACIONAL: z.enum(["1", "2", "3"]).default("3"),
  // regApTribSN (only when opSimpNac=3)
  NFSE_REG_AP_TRIB_SN: z.enum(["1", "2", "3"]).default("1"),
  NFSE_ISS_RATE_PERCENT: z.coerce.number().default(0),
  // pTotTribSN (only when opSimpNac≠1)
  NFSE_TOT_TRIB_SN_PERCENT: z.coerce.number().default(6),
  NFSE_SERIES: z.string().default("70000"),
  // last nDPS already used in this series
  NFSE_DPS_OFFSET: z.coerce.number().int().nonnegative().default(0),
  COMPANY_PHONE: z.string().optional(),
  COMPANY_EMAIL: z.string().optional(),
  // Banco Inter
  INTER_ENV: z.enum(["sandbox", "producao"]).default("sandbox"),
  INTER_CLIENT_ID: z.string().optional(),
  INTER_CLIENT_SECRET: z.string().optional(),
  INTER_CERT_PATH: z.string().optional(),
  INTER_KEY_PATH: z.string().optional(),
  INTER_ACCOUNT: z.string().optional(), // x-conta-corrente header
  INTER_BRANCH: z.string().default("0001-9"), // shown on the exported statement
  // Camim (delivery of NFS-e + boleto)
  CAMIM_BASE_URL: z.string().default("https://pj.camim.com.br/api"),
  CAMIM_TOKEN: z.string().optional(),
});

export const env = schema.parse(process.env);
export type Env = z.infer<typeof schema>;
