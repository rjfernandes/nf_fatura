import { describe, expect, it } from "vitest";
import { parseNfse } from "./parseNfse.js";

const xml = `<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse"><infNFSe Id="NFS123">
<nNFSe>42</nNFSe><dhProc>2026-10-01T00:10:00-03:00</dhProc>
<emit><CNPJ>11222333000181</CNPJ></emit><valores><vLiq>1234.50</vLiq></valores>
<DPS><infDPS><dhEmi>2026-09-30T23:50:00-03:00</dhEmi>
<toma><CNPJ>99888777000166</CNPJ><xNome>Cliente</xNome></toma></infDPS></DPS>
</infNFSe></NFSe>`;

describe("parseNfse", () => {
  it("reads key, number, taker, amount and keeps the emission date as written", () => {
    expect(parseNfse(xml)).toMatchObject({
      accessKey: "123",
      number: "42",
      issuedAt: "2026-09-30T23:50:00-03:00",
      takerTaxId: "99888777000166",
      takerName: "Cliente",
      amountCents: 123450,
      providerTaxId: "11222333000181",
    });
  });
});
