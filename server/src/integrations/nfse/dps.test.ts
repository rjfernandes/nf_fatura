import forge from "node-forge";
import { describe, expect, it } from "vitest";
import { buildDpsXml, dpsId, type DpsConfig } from "./dps.js";
import { signDps } from "./sign.js";

const cfg: DpsConfig = {
  tpAmb: 2,
  providerCnpj: "11222333000181",
  providerCityIbge: "3550308",
  serviceCode: "010101",
  serviceDescription: "Serviços & suporte",
  opSimpNac: "1",
  issRatePercent: 2,
  now: new Date("2026-09-28T13:00:00Z"),
};

const req = {
  billingId: "b1",
  dpsNumber: 7,
  amountCents: 123456,
  competence: "2026-09",
  customer: {
    name: "ACME <Ltda>",
    cnpj: "22333444000181",
    address: "Rua A",
    addressNumber: "10",
    addressComplement: null,
    neighborhood: "Centro",
    city: "SP",
    cityIbgeCode: "3550308",
    state: "SP",
    zipcode: "01001000",
  },
};

describe("DPS", () => {
  it("builds a 45-char Id", () => {
    const id = dpsId(cfg, 7);
    expect(id).toHaveLength(45);
    expect(id).toBe("DPS3550308211222333000181" + "00001" + "000000000000007");
  });
  it("escapes text, formats money and time", () => {
    const xml = buildDpsXml(cfg, req);
    expect(xml).toContain("<xNome>ACME &lt;Ltda&gt;</xNome>");
    expect(xml).toContain("<vServ>1234.56</vServ>");
    expect(xml).toContain("<dhEmi>2026-09-28T10:00:00-03:00</dhEmi>");
    expect(xml).not.toContain("<xCpl>");
  });
  it("matches the Emissor Web layout for a Simples (ME/EPP) provider", () => {
    const xml = buildDpsXml(
      {
        ...cfg,
        providerCnpj: "03894412000100",
        providerCityIbge: "3303302",
        providerPhone: "48991491080",
        providerEmail: "a@b.com",
        serviceCode: "010701",
        municipalServiceCode: "001",
        nbsCode: "115013000",
        serviceDescription:
          "Serviços de informática prestados Valor aproximado dos tributos: 6%",
        opSimpNac: "3",
        regApTribSN: "1",
        totTribSNPercent: 6,
        series: "70000",
      },
      { ...req, customer: { ...req.customer, cityIbgeCode: "3300803" } },
    );
    expect(xml).toContain("<serie>70000</serie>");
    expect(xml).toContain("<fone>48991491080</fone><email>a@b.com</email>");
    expect(xml).toContain(
      "<opSimpNac>3</opSimpNac><regApTribSN>1</regApTribSN><regEspTrib>0</regEspTrib>",
    );
    expect(xml).toContain("<cLocPrestacao>3300803</cLocPrestacao>");
    expect(xml).toContain(
      "<cTribNac>010701</cTribNac><cTribMun>001</cTribMun>",
    );
    expect(xml).toContain("</xDescServ><cNBS>115013000</cNBS>");
    expect(xml).toContain(
      "<tribFed><piscofins><CST>00</CST><tpRetPisCofins>0</tpRetPisCofins></piscofins></tribFed>",
    );
    expect(xml).toContain("<totTrib><pTotTribSN>6.00</pTotTribSN></totTrib>");
    expect(xml).not.toContain("<pAliq>");
  });
  it("requires the customer IBGE code", () => {
    expect(() =>
      buildDpsXml(cfg, {
        ...req,
        customer: { ...req.customer, cityIbgeCode: null },
      }),
    ).toThrow(/IBGE/);
  });
  it("signs infDPS with an enveloped signature", () => {
    const keys = forge.pki.rsa.generateKeyPair(1024);
    const cert = forge.pki.createCertificate();
    cert.publicKey = keys.publicKey;
    cert.serialNumber = "01";
    cert.validity.notBefore = new Date();
    cert.validity.notAfter = new Date(Date.now() + 1e9);
    cert.setSubject([{ name: "commonName", value: "t" }]);
    cert.setIssuer([{ name: "commonName", value: "t" }]);
    cert.sign(keys.privateKey);
    const signed = signDps(buildDpsXml(cfg, req), dpsId(cfg, 7), {
      privateKeyPem: forge.pki.privateKeyToPem(keys.privateKey),
      certPem: forge.pki.certificateToPem(cert),
    });
    expect(signed).toContain("<Signature");
    expect(signed).toContain(`URI="#${dpsId(cfg, 7)}"`);
    expect(signed).toContain("X509Certificate");
  });
});
