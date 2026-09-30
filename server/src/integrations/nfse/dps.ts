import type { NfseRequest } from "../types.js";

export interface DpsConfig {
  tpAmb: 1 | 2; // 1 produção, 2 homologação
  providerCnpj: string;
  providerMunicipalRegistration?: string;
  providerCityIbge: string;
  providerPhone?: string;
  providerEmail?: string;
  serviceCode: string; // cTribNac
  municipalServiceCode?: string; // cTribMun
  nbsCode?: string; // cNBS
  serviceDescription: string;
  opSimpNac: "1" | "2" | "3";
  regApTribSN?: "1" | "2" | "3"; // only sent when opSimpNac = 3 (ME/EPP)
  issRatePercent: number;
  totTribSNPercent?: number; // pTotTribSN, only sent when opSimpNac ≠ 1
  series?: string;
  now?: Date;
}

const esc = (s: string) =>
  s.replace(
    /[<>&'"]/g,
    (c) =>
      ({
        "<": "&lt;",
        ">": "&gt;",
        "&": "&amp;",
        "'": "&apos;",
        '"': "&quot;",
      })[c]!,
  );
const money = (cents: number) => (cents / 100).toFixed(2);

/**
 * DPS Id = "DPS" + cLocEmi(7) + tpInsc(1) + CNPJ(14) + serie(5) + nDPS(15) = 45
 * chars.
 */
export function dpsId(cfg: DpsConfig, nDPS: number): string {
  const serie = (cfg.series ?? "1").padStart(5, "0");
  return `DPS${cfg.providerCityIbge}2${cfg.providerCnpj}${serie}${String(nDPS).padStart(15, "0")}`;
}

/** Brasília-time ISO timestamp with -03:00 offset. */
function brtIso(d: Date): string {
  const shifted = new Date(d.getTime() - 3 * 3600_000);
  return shifted.toISOString().replace(/\.\d+Z$/, "-03:00");
}

export function buildDpsXml(cfg: DpsConfig, req: NfseRequest): string {
  const c = req.customer;
  const now = cfg.now ?? new Date();
  const id = dpsId(cfg, req.dpsNumber);
  const tomaMun = c.cityIbgeCode;
  if (!tomaMun) {
    throw new Error(
      `Cliente ${c.name}: código IBGE do município é obrigatório para emitir NFS-e`,
    );
  }
  const im = cfg.providerMunicipalRegistration
    ? `<IM>${esc(cfg.providerMunicipalRegistration)}</IM>`
    : "";
  const cpl = c.addressComplement
    ? `<xCpl>${esc(c.addressComplement)}</xCpl>`
    : "";
  const simples = cfg.opSimpNac !== "1";
  const fone = cfg.providerPhone
    ? `<fone>${esc(cfg.providerPhone)}</fone>`
    : "";
  const email = cfg.providerEmail
    ? `<email>${esc(cfg.providerEmail)}</email>`
    : "";
  const regApTribSN =
    cfg.opSimpNac === "3" && cfg.regApTribSN
      ? `<regApTribSN>${cfg.regApTribSN}</regApTribSN>`
      : "";
  const cTribMun = cfg.municipalServiceCode
    ? `<cTribMun>${esc(cfg.municipalServiceCode)}</cTribMun>`
    : "";
  const cNBS = cfg.nbsCode ? `<cNBS>${esc(cfg.nbsCode)}</cNBS>` : "";
  const iss = simples
    ? `<tribMun><tribISSQN>1</tribISSQN><tpRetISSQN>1</tpRetISSQN></tribMun>`
    : `<tribMun><tribISSQN>1</tribISSQN><pAliq>${cfg.issRatePercent.toFixed(2)}</pAliq><tpRetISSQN>1</tpRetISSQN></tribMun>`;
  const tribFed = simples
    ? `<tribFed><piscofins><CST>00</CST><tpRetPisCofins>0</tpRetPisCofins></piscofins></tribFed>`
    : "";
  const totTrib =
    simples && cfg.totTribSNPercent !== undefined
      ? `<totTrib><pTotTribSN>${cfg.totTribSNPercent.toFixed(2)}</pTotTribSN></totTrib>`
      : `<totTrib><indTotTrib>0</indTotTrib></totTrib>`;

  return (
    `<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01">` +
    `<infDPS Id="${id}">` +
    `<tpAmb>${cfg.tpAmb}</tpAmb>` +
    `<dhEmi>${brtIso(now)}</dhEmi>` +
    `<verAplic>nf-fatura-1.0</verAplic>` +
    `<serie>${Number(cfg.series ?? "1")}</serie>` +
    `<nDPS>${req.dpsNumber}</nDPS>` +
    `<dCompet>${req.competence}-01</dCompet>` +
    `<tpEmit>1</tpEmit>` +
    `<cLocEmi>${cfg.providerCityIbge}</cLocEmi>` +
    `<prest><CNPJ>${cfg.providerCnpj}</CNPJ>${im}${fone}${email}<regTrib><opSimpNac>${cfg.opSimpNac}</opSimpNac>${regApTribSN}<regEspTrib>0</regEspTrib></regTrib></prest>` +
    `<toma><CNPJ>${c.cnpj}</CNPJ><xNome>${esc(c.name)}</xNome>` +
    `<end><endNac><cMun>${tomaMun}</cMun><CEP>${c.zipcode}</CEP></endNac>` +
    `<xLgr>${esc(c.address)}</xLgr><nro>${esc(c.addressNumber)}</nro>${cpl}<xBairro>${esc(c.neighborhood)}</xBairro></end></toma>` +
    `<serv><locPrest><cLocPrestacao>${tomaMun}</cLocPrestacao></locPrest>` +
    `<cServ><cTribNac>${cfg.serviceCode}</cTribNac>${cTribMun}<xDescServ>${esc(cfg.serviceDescription)}</xDescServ>${cNBS}</cServ></serv>` +
    `<valores><vServPrest><vServ>${money(req.amountCents)}</vServ></vServPrest>` +
    `<trib>${iss}${tribFed}${totTrib}</trib></valores>` +
    `</infDPS></DPS>`
  );
}
