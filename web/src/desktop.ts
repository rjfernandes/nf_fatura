// The desktop app (Tauri): the same operations as the HTTP API, answered by
// the Rust backend through commands, plus what only the desktop has.
import {
  invoke,
  type InvokeArgs,
  type InvokeOptions,
} from "@tauri-apps/api/core";
import type { Api, Billing } from "./api";

export const isDesktop = "__TAURI_INTERNALS__" in window;

/** Commands fail with { status, message }, as the HTTP API does. */
export async function run<T>(
  command: string,
  args?: InvokeArgs,
  options?: InvokeOptions,
): Promise<T> {
  try {
    return await invoke<T>(command, args, options);
  } catch (e) {
    const message =
      typeof e === "string" ? e : (e as { message?: string })?.message;
    throw new Error(message || "Erro inesperado");
  }
}

export const desktopApi: Api = {
  customers: () => run("customers_list"),
  createCustomer: (input) => run("customers_create", { input }),
  updateCustomer: (id, input) => run("customers_update", { id, input }),
  deleteCustomer: (id) => run("customers_delete", { id }),
  billings: () => run("billings_list"),
  createBillings: (input) => run("billings_create", { input }),
  statement: (competence) => run("statement_summary", { competence }),
  deleteSlip: (id) => run("slip_cancel", { id }),
  deleteBilling: (id) => run("billing_delete", { id }),
  importDocuments: (from) => run("billings_import", { from }),
  retry: (id) => run("billing_retry", { id }),
  // The PDF goes as the raw body; headers only take ASCII.
  attachNfse: async (id, file, number) =>
    run<Billing>("nfse_attach", new Uint8Array(await file.arrayBuffer()), {
      headers: {
        "x-billing-id": id,
        ...(number ? { "x-nfse-number": encodeURIComponent(number) } : {}),
      },
    }),
  deleteNfse: (id) => run("nfse_remove", { id }),
  integrations: () => run("integrations_list"),
  sendDelivery: (v) => run("delivery_send", { id: v.id, parts: v.parts }),
};

export interface Settings {
  nfseEnv: "homologacao" | "producao";
  nfseCertPfxPath: string | null;
  companyCnpj: string | null;
  companyName: string;
  companyMunicipalRegistration: string | null;
  companyCityIbge: string | null;
  companyPhone: string | null;
  companyEmail: string | null;
  nfseServiceCode: string;
  nfseMunicipalServiceCode: string;
  nfseNbsCode: string;
  nfseServiceDescription: string;
  nfseSimplesNacional: "1" | "2" | "3";
  nfseRegApTribSn: "1" | "2" | "3";
  nfseIssRatePercent: number;
  nfseTotTribSnPercent: number;
  nfseSeries: string;
  nfseDpsOffset: number;
  interEnv: "sandbox" | "producao";
  interClientId: string | null;
  interCertPath: string | null;
  interKeyPath: string | null;
  interAccount: string | null;
  interBranch: string;
  camimBaseUrl: string;
}

/** Secrets never come back from the app: only whether each one is stored. */
export interface SettingsView extends Settings {
  hasNfseCertPfxPassword: boolean;
  hasInterClientSecret: boolean;
  hasCamimToken: boolean;
  databasePath: string;
}

/** undefined keeps the stored secret, "" removes it. */
export interface SecretsUpdate {
  nfseCertPfxPassword?: string;
  interClientSecret?: string;
  camimToken?: string;
}

export const settingsApi = {
  get: () => run<SettingsView>("settings_get"),
  save: (settings: Settings, secrets: SecretsUpdate) =>
    run<SettingsView>("settings_save", { settings, secrets }),
  importEnv: (path: string) =>
    run<SettingsView>("settings_import_env", { path }),
  importDatabase: (path: string) =>
    run<{ backup: string | null }>("db_import", { path }),
  pickFile: (title: string, extensions?: string[]) =>
    run<string | null>("pick_file", { title, extensions }),
};
