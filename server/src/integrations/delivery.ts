import type { Env } from "../config/env.js";
import { CamimDeliveryProvider } from "./camim/camimClient.js";
import type { DeliveryProvider } from "./types.js";

/** Integrations a customer can be enabled for (Customer.integration). */
export const INTEGRATIONS = ["CAMIM"] as const;
export type IntegrationKey = (typeof INTEGRATIONS)[number];
export type DeliveryProviders = Record<IntegrationKey, DeliveryProvider>;

export const buildDeliveryProviders = (env: Env): DeliveryProviders => ({
  CAMIM: new CamimDeliveryProvider(env),
});
