import { XMLParser } from "fast-xml-parser";
import type { ImportedNfse } from "../types.js";

const parser = new XMLParser({
  removeNSPrefix: true,
  parseTagValue: false,
  ignoreAttributes: false,
  attributeNamePrefix: "@_",
});

/**
 * Reads an authorized NFS-e XML. The emission date is kept as written (with
 * its offset) so the competence comes from the date the issuer saw, not from a
 * timezone conversion.
 */
export function parseNfse(
  xml: string,
): ImportedNfse & { providerTaxId: string } {
  const inf = parser.parse(xml).NFSe.infNFSe;
  const dps = inf.DPS.infDPS;
  const taker = dps.toma ?? {};
  const emit = inf.emit ?? {};
  const value = inf.valores?.vLiq ?? dps.valores?.vServPrest?.vServ ?? "0";
  return {
    accessKey: String(inf["@_Id"] ?? "").replace(/^NFS/, ""),
    number: String(inf.nNFSe ?? ""),
    xml,
    issuedAt: String(dps.dhEmi ?? inf.dhProc),
    takerTaxId: String(taker.CNPJ ?? taker.CPF ?? ""),
    takerName: taker.xNome ? String(taker.xNome) : undefined,
    takerAddress: {
      address: taker.end?.xLgr,
      number: taker.end?.nro,
      complement: taker.end?.xCpl,
      neighborhood: taker.end?.xBairro,
      cityIbgeCode: taker.end?.endNac?.cMun,
      zipcode: taker.end?.endNac?.CEP,
    },
    amountCents: Math.round(Number(value) * 100),
    providerTaxId: String(emit.CNPJ ?? emit.CPF ?? ""),
  };
}
