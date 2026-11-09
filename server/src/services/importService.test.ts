import { beforeEach, describe, expect, it } from "vitest";
import { createPrisma } from "../db.js";
import type { ImportedNfse, ImportedSlip } from "../integrations/types.js";
import { ImportService } from "./importService.js";

const db = createPrisma("file:./prisma/test.db");

const note: ImportedNfse = {
  accessKey: "K1",
  number: "10",
  xml: "<x/>",
  issuedAt: "2026-10-01T11:22:50-03:00",
  competence: "2026-10",
  takerTaxId: "11222333000181",
  takerName: "Acme",
  amountCents: 5000,
};
const slip: ImportedSlip = {
  codigoSolicitacao: "S1",
  dueDate: "2026-10-03",
  amountCents: 5000,
  payer: { taxId: "11.222.333/0001-81", name: "Acme" },
};

const service = (notes: ImportedNfse[], slips: ImportedSlip[]) =>
  new ImportService(
    db,
    { issue: async () => ({}), list: async () => notes },
    {
      create: async () => ({ codigoSolicitacao: "" }),
      cancel: async () => {},
      list: async () => slips,
    },
  );

beforeEach(async () => {
  await db.delivery.deleteMany();
  await db.bankSlip.deleteMany();
  await db.invoice.deleteMany();
  await db.billing.deleteMany();
  await db.customer.deleteMany();
});

describe("ImportService", () => {
  it.each([["nfse first"], ["slip first"]])(
    "joins the NFS-e and the boleto of the same competence (%s)",
    async (order) => {
      const svc = service([note], [slip]);
      const steps = [() => svc.importFromNfse(), () => svc.importFromSlips()];
      if (order === "slip first") {
        steps.reverse();
      }
      const first = await steps[0]();
      const second = await steps[1]();
      expect(first).toMatchObject({ imported: 1, createdCustomers: 1 });
      expect(second).toMatchObject({ linked: 1, createdCustomers: 0 });
      const billings = await db.billing.findMany({
        include: { invoice: true, bankSlip: true },
      });
      expect(billings).toHaveLength(1);
      // Issued and due in October for the service of September.
      expect(billings[0]).toMatchObject({
        competence: "2026-09",
        status: "COMPLETED",
        issueNfse: true,
        issueSlip: true,
      });
      expect(billings[0].invoice?.source).toBe("IMPORTED");
      expect(billings[0].bankSlip?.codigoSolicitacao).toBe("S1");
      const customer = await db.customer.findFirstOrThrow();
      expect(customer).toMatchObject({
        cnpj: "11222333000181",
        recurringValue: 5000,
      });
    },
  );

  it("completes a note attached by hand instead of duplicating it", async () => {
    await db.customer.create({
      data: {
        name: "Acme",
        cnpj: "11222333000181",
        address: "Rua A",
        addressNumber: "1",
        neighborhood: "Centro",
        city: "SP",
        state: "SP",
        zipcode: "01001000",
        recurringValue: 5000,
        billings: {
          create: {
            competence: "2026-09",
            amountCents: 5000,
            dueDate: "2026-10-03",
            issueNfse: true,
            status: "COMPLETED",
            invoice: { create: { number: "10", source: "MANUAL" } },
          },
        },
      },
    });
    expect(await service([note], []).importFromNfse()).toMatchObject({
      duplicates: 1,
      imported: 0,
    });
    expect(await db.billing.count()).toBe(1);
    expect(await db.invoice.findFirst()).toMatchObject({
      accessKey: "K1",
      xml: "<x/>",
      source: "MANUAL",
    });
  });

  it("does not import the same documents twice", async () => {
    const svc = service([note], [slip]);
    await svc.importFromNfse();
    await svc.importFromSlips();
    expect(await svc.importFromNfse()).toMatchObject({
      duplicates: 1,
      imported: 0,
    });
    expect(await svc.importFromSlips()).toMatchObject({
      duplicates: 1,
      imported: 0,
    });
  });
});
