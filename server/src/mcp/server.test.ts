import { mkdtemp, readFile, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { describe, expect, it } from "vitest";
import { createPrisma } from "../db.js";
import type {
  NfseProvider,
  SlipProvider,
  StatementProvider,
} from "../integrations/types.js";
import { BillingService } from "../services/billingService.js";
import { StatementService } from "../services/statementService.js";
import { buildMcpServer } from "./server.js";

const provider: StatementProvider = {
  fetch: async () => [
    {
      id: "1",
      date: "2026-08-03",
      at: "2026-08-03T09:00:00",
      type: "BOLETO_COBRANCA",
      operation: "C",
      amountCents: 2425500,
      title: "Boleto de cobranca recebido",
      description: "112/90801485353",
      counterpart: "CAMIM ANCHIETA",
    },
  ],
  balance: async () => ({ available: 2426008, blocked: 0 }),
};
const account = {
  companyName: "MRF INFORMATICA LTDA",
  cnpj: "03894412000100",
  branch: "0001-9",
  account: "143906330",
};

const db = createPrisma("file:./prisma/test.db");
const nfse: NfseProvider = {
  issue: async () => ({
    number: "7",
    pdf: Buffer.from("%PDF-nfse"),
  }),
};
const slip: SlipProvider = {
  create: async () => ({
    codigoSolicitacao: "n",
    linhaDigitavel: "999",
    pdf: Buffer.from("%PDF-new"),
  }),
  cancel: async () => {},
};

async function connect(dir: string) {
  const server = buildMcpServer(
    new StatementService(provider, account),
    new BillingService(db, nfse, slip),
    db,
    dir,
  );
  const client = new Client({ name: "test", version: "1" });
  const [a, b] = InMemoryTransport.createLinkedPair();
  await Promise.all([server.connect(a), client.connect(b)]);
  return client;
}

const textOf = (r: any) => r.content[0].text as string;

describe("MCP statement server", () => {
  it("lists both tools", async () => {
    const client = await connect(".");
    const { tools } = await client.listTools();
    expect(tools.map((t) => t.name).sort()).toEqual([
      "export_statement",
      "get_billing_documents",
      "get_statement",
      "issue_billing_documents",
    ]);
  });

  it("get_statement summarises the month", async () => {
    const client = await connect(".");
    const r = await client.callTool({
      name: "get_statement",
      arguments: { competence: "2026-08" },
    });
    expect(textOf(r)).toContain("Entradas:");
    expect(textOf(r)).toContain("Boleto de cobranca recebido: 112/90801485353");
  });

  it("export_statement writes PDF and OFX by default", async () => {
    const dir = await mkdtemp(join(tmpdir(), "extrato-"));
    const client = await connect(dir);
    await client.callTool({
      name: "export_statement",
      arguments: { competence: "2026-08" },
    });
    expect((await readdir(dir)).sort()).toEqual([
      "Extrato-01-08-2026-a-31-08-2026-OFX.ofx",
      "Extrato-01-08-2026-a-31-08-2026-PDF.pdf",
    ]);
    expect(
      (await readFile(join(dir, "Extrato-01-08-2026-a-31-08-2026-PDF.pdf")))
        .subarray(0, 5)
        .toString(),
    ).toBe("%PDF-");
  });

  it("export_statement honours the requested format", async () => {
    const dir = await mkdtemp(join(tmpdir(), "extrato-"));
    const client = await connect(dir);
    await client.callTool({
      name: "export_statement",
      arguments: { competence: "2026-08", formats: ["ofx"] },
    });
    expect(await readdir(dir)).toEqual([
      "Extrato-01-08-2026-a-31-08-2026-OFX.ofx",
    ]);
  });

  it("reports a future month as a tool error", async () => {
    const client = await connect(".");
    const r = await client.callTool({
      name: "get_statement",
      arguments: { competence: "2999-01" },
    });
    expect(r.isError).toBe(true);
    expect(textOf(r)).toContain("mês atual ou um anterior");
  });
});

describe("MCP billing documents", () => {
  const base = {
    address: "Rua A",
    addressNumber: "1",
    neighborhood: "Centro",
    city: "SP",
    state: "SP",
    zipcode: "01001000",
    recurringValue: 10000,
  };

  async function seed(withSlip: boolean) {
    await db.bankSlip.deleteMany();
    await db.invoice.deleteMany();
    await db.dpsSequence.deleteMany();
    await db.billing.deleteMany();
    await db.customer.deleteMany();
    const c = await db.customer.create({
      data: { ...base, name: "ACME Ltda", cnpj: "11222333000181" },
    });
    await db.billing.create({
      data: {
        customerId: c.id,
        competence: "2026-08",
        amountCents: 10000,
        dueDate: "2026-09-10",
        bankSlip: withSlip
          ? {
              create: {
                codigoSolicitacao: "x",
                linhaDigitavel: "123",
                pdf: Buffer.from("%PDF-1.4 slip"),
              },
            }
          : undefined,
      },
    });
  }

  const get = (client: any, args: object) =>
    client.callTool({
      name: "get_billing_documents",
      arguments: { company: "acme", competence: "2026-08", ...args },
    });

  it("saves the slip PDF found by name or CNPJ", async () => {
    await seed(true);
    const dir = await mkdtemp(join(tmpdir(), "boleto-"));
    const client = await connect(dir);
    for (const company of ["acme", "11.222.333/0001-81"]) {
      const r = await get(client, { company, documents: ["slip"] });
      expect(r.isError).toBeFalsy();
      expect(textOf(r)).toContain("Linha digitável: 123");
    }
    expect(await readdir(dir)).toEqual(["Boleto-ACME-Ltda-2026-08.pdf"]);
  });

  it("says what is missing and asks instead of issuing", async () => {
    await seed(true);
    const client = await connect(await mkdtemp(join(tmpdir(), "doc-")));
    const r = await get(client, {});
    expect(textOf(r)).toContain("Linha digitável: 123");
    expect(textOf(r)).toContain("Não existe nota fiscal (NFS-e)");
    expect(textOf(r)).toContain("Pergunte ao usuário");
    expect(await db.invoice.count()).toBe(0);
  });

  it("reports a competence without billing", async () => {
    await seed(true);
    const client = await connect(".");
    const r = await get(client, { competence: "2026-07" });
    expect(textOf(r)).toContain("Não existe boleto nem nota fiscal");
  });

  it("issue_billing_documents issues only what is missing", async () => {
    await seed(true);
    const dir = await mkdtemp(join(tmpdir(), "issue-"));
    const client = await connect(dir);
    const r = await client.callTool({
      name: "issue_billing_documents",
      arguments: {
        company: "acme",
        competence: "2026-08",
        documents: ["nfse"],
      },
    });
    expect(textOf(r)).toContain("Nota fiscal emitida");
    expect(await readdir(dir)).toEqual(["NFSe-ACME-Ltda-2026-08.pdf"]);
    expect(await db.billing.count()).toBe(1);
  });

  it("issue_billing_documents requires a due date for the slip", async () => {
    await seed(false);
    const client = await connect(".");
    const r = await client.callTool({
      name: "issue_billing_documents",
      arguments: {
        company: "acme",
        competence: "2026-08",
        documents: ["slip"],
      },
    });
    expect(r.isError).toBe(true);
    expect(textOf(r)).toContain("due_date");
  });
});
