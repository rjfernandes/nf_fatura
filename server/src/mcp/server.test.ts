import { mkdtemp, readFile, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { describe, expect, it } from "vitest";
import type { StatementProvider } from "../integrations/types.js";
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

async function connect(dir: string) {
  const server = buildMcpServer(new StatementService(provider, account), dir);
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
      "get_statement",
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
