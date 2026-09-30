import { beforeEach, describe, expect, it } from "vitest";
import { createPrisma } from "../db.js";
import { BillingService } from "./billingService.js";

const db = createPrisma("file:./prisma/test.db");

const base = {
  address: "Rua A",
  addressNumber: "1",
  neighborhood: "Centro",
  city: "SP",
  state: "SP",
  zipcode: "01001000",
  recurringValue: 10000,
};

beforeEach(async () => {
  await db.bankSlip.deleteMany();
  await db.invoice.deleteMany();
  await db.billing.deleteMany();
  await db.customer.deleteMany();
});

describe("BillingService", () => {
  it("isolates failures per customer and creates slips only when enabled", async () => {
    const ok = await db.customer.create({
      data: { ...base, name: "OK", cnpj: "11222333000181", hasBankSlip: true },
    });
    const noSlip = await db.customer.create({
      data: {
        ...base,
        name: "NoSlip",
        cnpj: "22333444000181",
        hasBankSlip: false,
      },
    });
    const bad = await db.customer.create({
      data: { ...base, name: "Bad", cnpj: "33444555000181", hasBankSlip: true },
    });
    let slips = 0;
    const svc = new BillingService(
      db,
      {
        issue: async (r) => {
          if (r.customer.name === "Bad") {
            throw new Error("boom");
          }
          return { number: "1", xml: "<x/>" };
        },
      },
      {
        create: async () => {
          slips++;
          return { codigoSolicitacao: "abc" };
        },
        cancel: async () => {},
      },
    );
    const res = await svc.run({
      customerIds: [ok.id, noSlip.id, bad.id],
      competence: "2026-09",
      dueDate: "2026-10-10",
    });
    const by = Object.fromEntries(res.map((b) => [b.customer.name, b]));
    expect(by.OK.status).toBe("COMPLETED");
    expect(by.NoSlip.status).toBe("COMPLETED");
    expect(by.Bad.status).toBe("FAILED");
    expect(by.Bad.error).toBe("boom");
    expect(slips).toBe(1);
  });

  it.each([
    ["NFSE", 1, 0],
    ["SLIP", 0, 1],
    ["BOTH", 1, 1],
    ["AUTO", 1, 0], // customer has hasBankSlip=false
  ] as const)(
    "mode %s issues %i NFS-e and %i boleto",
    async (mode, nfse, slip) => {
      const c = await db.customer.create({
        data: {
          ...base,
          name: "M",
          cnpj: "11222333000181",
          hasBankSlip: false,
        },
      });
      let issued = 0,
        slips = 0;
      const svc = new BillingService(
        db,
        {
          issue: async () => {
            issued++;
            return { number: "1" };
          },
        },
        {
          create: async () => {
            slips++;
            return { codigoSolicitacao: "a" };
          },
          cancel: async () => {},
        },
      );
      const [b] = await svc.run({
        customerIds: [c.id],
        competence: "2026-09",
        dueDate: "2026-10-10",
        mode,
      });
      expect(b.status).toBe("COMPLETED");
      expect([issued, slips]).toEqual([nfse, slip]);
    },
  );

  it("retry resumes without re-issuing the NFS-e", async () => {
    const c = await db.customer.create({
      data: { ...base, name: "R", cnpj: "11222333000181", hasBankSlip: true },
    });
    let issued = 0,
      fail = true;
    const svc = new BillingService(
      db,
      {
        issue: async () => {
          issued++;
          return { number: "9" };
        },
      },
      {
        create: async () => {
          if (fail) {
            throw new Error("inter down");
          }
          return { codigoSolicitacao: "z" };
        },
        cancel: async () => {},
      },
    );
    const [b] = await svc.run({
      customerIds: [c.id],
      competence: "2026-09",
      dueDate: "2026-10-10",
    });
    expect(b.status).toBe("FAILED");
    fail = false;
    await svc.process(b.id);
    expect(
      (await db.billing.findUniqueOrThrow({ where: { id: b.id } })).status,
    ).toBe("COMPLETED");
    expect(issued).toBe(1);
  });

  it("cancelSlip cancels at the bank, drops the local slip and stops retries from recreating it", async () => {
    const c = await db.customer.create({
      data: { ...base, name: "C", cnpj: "11222333000181", hasBankSlip: true },
    });
    const cancelled: string[] = [];
    let refuse = true;
    const svc = new BillingService(
      db,
      { issue: async () => ({ number: "1" }) },
      {
        create: async () => ({ codigoSolicitacao: "cod1" }),
        cancel: async (cod) => {
          if (refuse) {
            throw new Error("já pago");
          }
          cancelled.push(cod);
        },
      },
    );
    const [b] = await svc.run({
      customerIds: [c.id],
      competence: "2026-09",
      dueDate: "2026-10-10",
    });
    await expect(svc.cancelSlip(b.id)).rejects.toThrow("já pago");
    // bank refused: nothing changed
    expect(await db.bankSlip.count({ where: { billingId: b.id } })).toBe(1);
    refuse = false;
    // has an NFS-e: billing stays
    expect(await svc.cancelSlip(b.id)).toEqual({ billingRemoved: false });
    expect(cancelled).toEqual(["cod1"]);
    expect(await db.bankSlip.count({ where: { billingId: b.id } })).toBe(0);
    expect(
      (await db.billing.findUniqueOrThrow({ where: { id: b.id } })).issueSlip,
    ).toBe(false);
  });

  it("cancelSlip also removes a boleto-only billing from the history", async () => {
    const c = await db.customer.create({
      data: { ...base, name: "S", cnpj: "11222333000181", hasBankSlip: true },
    });
    const svc = new BillingService(
      db,
      { issue: async () => ({ number: "1" }) },
      {
        create: async () => ({ codigoSolicitacao: "cod2" }),
        cancel: async () => {},
      },
    );
    const [b] = await svc.run({
      customerIds: [c.id],
      competence: "2026-09",
      dueDate: "2026-10-10",
      mode: "SLIP",
    });
    expect(await svc.cancelSlip(b.id)).toEqual({ billingRemoved: true });
    expect(await db.billing.count({ where: { id: b.id } })).toBe(0);
  });
});
