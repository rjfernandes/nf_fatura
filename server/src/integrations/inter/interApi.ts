import { readFileSync } from "node:fs";
import { Agent, request } from "undici";
import type { Env } from "../../config/env.js";

const BASE = {
  sandbox: "https://cdpj-sandbox.partners.uatinter.co",
  producao: "https://cdpj.partners.bancointer.com.br",
} as const;

/**
 * Shared mTLS + OAuth2 plumbing for every Banco Inter API (boleto, extrato...).
 */
export class InterApi {
  private agent?: Agent;
  // One token per scope set, so a missing scope on the integration only breaks
  // the feature that needs it.
  private tokens = new Map<string, { value: string; expiresAt: number }>();

  constructor(private env: Env) {}

  private get base() {
    return BASE[this.env.INTER_ENV];
  }

  private getAgent() {
    const { INTER_CERT_PATH, INTER_KEY_PATH } = this.env;
    if (!INTER_CERT_PATH || !INTER_KEY_PATH) {
      throw new Error(
        "Banco Inter: INTER_CERT_PATH/INTER_KEY_PATH não configurados",
      );
    }
    this.agent ??= new Agent({
      connect: {
        cert: readFileSync(INTER_CERT_PATH),
        key: readFileSync(INTER_KEY_PATH),
      },
    });
    return this.agent;
  }

  private async getToken(scope: string): Promise<string> {
    const cached = this.tokens.get(scope);
    if (cached && cached.expiresAt > Date.now() + 30_000) {
      return cached.value;
    }
    const { INTER_CLIENT_ID, INTER_CLIENT_SECRET } = this.env;
    if (!INTER_CLIENT_ID || !INTER_CLIENT_SECRET) {
      throw new Error(
        "Banco Inter: INTER_CLIENT_ID/INTER_CLIENT_SECRET não configurados",
      );
    }
    const res = await request(`${this.base}/oauth/v2/token`, {
      method: "POST",
      dispatcher: this.getAgent(),
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        client_id: INTER_CLIENT_ID,
        client_secret: INTER_CLIENT_SECRET,
        grant_type: "client_credentials",
        scope,
      }).toString(),
    });
    const json: any = await res.body.json();
    if (res.statusCode !== 200) {
      throw new Error(
        `Banco Inter auth ${res.statusCode}: ${JSON.stringify(json)}`,
      );
    }
    const token = {
      value: json.access_token as string,
      expiresAt: Date.now() + json.expires_in * 1000,
    };
    this.tokens.set(scope, token);
    return token.value;
  }

  async call<T>(
    scope: string,
    method: "GET" | "POST",
    path: string,
    body?: unknown,
  ): Promise<T> {
    const headers: Record<string, string> = {
      authorization: `Bearer ${await this.getToken(scope)}`,
      "content-type": "application/json",
    };
    if (this.env.INTER_ACCOUNT) {
      headers["x-conta-corrente"] = this.env.INTER_ACCOUNT;
    }
    const res = await request(`${this.base}${path}`, {
      method,
      dispatcher: this.getAgent(),
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });
    const text = await res.body.text();
    if (res.statusCode >= 300) {
      throw new Error(
        `Banco Inter ${method} ${path} ${res.statusCode}: ${text}`,
      );
    }
    return (text ? JSON.parse(text) : {}) as T;
  }
}
