import { XMLParser } from "fast-xml-parser";
import PDFDocument from "pdfkit";
import QRCode from "qrcode";

/**
 * Renders the DANFSe (auxiliary document of the NFS-e) from the authorized
 * NFS-e XML, following the layout of the national DANFSe v2.0.
 *
 * Coordinates below are in the units of the reference layout (1413 px wide);
 * the page is scaled down to A4.
 */

export interface DanfseExtras {
  /** The XML has no city name for the customer; the DB does. */
  customerCity?: string;
  customerState?: string;
}

const W = 1413;
const GRAY = "#f2f2f2";
const COL = [28, 372, 715, 1058];
const LEFT = 20;
const RIGHT = 1394;
const BOX_W = 344; // gray title box (LEFT..364)

type Node = Record<string, any>;

const parser = new XMLParser({
  removeNSPrefix: true,
  parseTagValue: false,
  ignoreAttributes: false,
  attributeNamePrefix: "@_",
});

const digits = (v?: string) => (v ?? "").replace(/\D/g, "");
const dash = (v?: string | null) => (v && v.trim() ? v.trim() : "-");

const cnpjCpf = (v?: string) => {
  const d = digits(v);
  if (d.length === 14) {
    return d.replace(/^(.{2})(.{3})(.{3})(.{4})(.{2})$/, "$1.$2.$3/$4-$5");
  }
  if (d.length === 11) {
    return d.replace(/^(.{3})(.{3})(.{3})(.{2})$/, "$1.$2.$3-$4");
  }
  return dash(v);
};
const phone = (v?: string) => {
  const d = digits(v);
  if (d.length === 11) {
    return d.replace(/^(.{2})(.{5})(.{4})$/, "($1) $2-$3");
  }
  if (d.length === 10) {
    return d.replace(/^(.{2})(.{4})(.{4})$/, "($1) $2-$3");
  }
  return dash(v);
};
const cep = (v?: string) =>
  digits(v).length === 8
    ? digits(v).replace(/^(.{2})(.{3})(.{3})$/, "$1.$2-$3")
    : dash(v);
const ibge = (v?: string) =>
  digits(v).length === 7
    ? digits(v).replace(/^(.{2})(.{5})$/, "$1.$2")
    : dash(v);
const brl = (v?: string) =>
  v === undefined
    ? "-"
    : Number(v).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });
const date = (iso?: string) =>
  iso ? iso.slice(0, 10).split("-").reverse().join("/") : "-";
const dateTime = (iso?: string) =>
  iso ? `${date(iso)} ${iso.slice(11, 19)}` : "-";
const address = (e?: Node) =>
  e ? [e.xLgr, e.nro, e.xCpl, e.xBairro].filter(Boolean).join(", ") : "-";

const SIMPLES: Record<string, string> = {
  "1": "Não Optante",
  "2": "Optante - Microempreendedor Individual (MEI)",
  "3": "Optante - Microempresa ou Empresa de Pequeno Porte (ME/EPP)",
};
const REG_AP: Record<string, string> = {
  "1": "Regime de apuração dos tributos federais e municipal pelo Simples Nacional",
  "2": "Regime de apuração dos tributos federais pelo Simples Nacional e ISSQN pela NFS-e conforme respectivas legislações municipais do tributo",
  "3": "Regime de apuração dos tributos federais e municipal pela NFS-e conforme respectivas legislações federal e municipal de cada tributo",
};
const RET_ISS: Record<string, string> = {
  "1": "Não Retido",
  "2": "Retido pelo Tomador",
  "3": "Retido pelo Intermediário",
};
const TRIB_ISS: Record<string, string> = {
  "1": "Operação Tributável",
  "2": "Imunidade",
  "3": "Exportação de Serviço",
  "4": "Não Incidência",
};
const PIS_COFINS_RET: Record<string, string> = {
  "0": "0 - PIS/COFINS/CSLL Não Retidos",
  "3": "3 - PIS/COFINS Retidos",
  "4": "4 - PIS/COFINS/CSLL Retidos",
};

export async function renderDanfse(
  xml: string,
  extras: DanfseExtras = {},
): Promise<Buffer> {
  const nfse = parser.parse(xml).NFSe as Node;
  const inf = nfse.infNFSe as Node;
  const dps = inf.DPS.infDPS as Node;
  const emit = inf.emit as Node;
  const toma = dps.toma as Node;
  const serv = dps.serv as Node;
  const trib = dps.valores.trib as Node;
  const chave = String(inf["@_Id"] ?? "").replace(/^NFS/, "");
  const regTrib = dps.prest?.regTrib as Node | undefined;

  const emitUf = emit.enderNac?.UF ?? "";
  const tomaCity = extras.customerCity ?? inf.xLocPrestacao;
  const tomaUf = extras.customerState ?? "";
  const tomaCityUf = [tomaCity, tomaUf].filter(Boolean).join(" / ");
  const pAliq = trib.tribMun?.pAliq;

  const doc = new PDFDocument({ size: "A4", margin: 0 });
  const chunks: Buffer[] = [];
  doc.on("data", (c: Buffer) => chunks.push(c));
  const done = new Promise<Buffer>((resolve) =>
    doc.on("end", () => resolve(Buffer.concat(chunks))),
  );
  doc.scale(doc.page.width / W);
  // The scaled layout runs past the unscaled page height, which would make
  // pdfkit auto-paginate text. The MediaBox is already written, so this only
  // affects that overflow check.
  doc.page.height = 1e6;

  doc.fillColor("black").strokeColor("black");
  const bold = (size: number) => doc.font("Helvetica-Bold").fontSize(size);
  const reg = (size: number) => doc.font("Helvetica").fontSize(size);
  const hline = (y: number, lw = 1) =>
    doc.lineWidth(lw).moveTo(LEFT, y).lineTo(RIGHT, y).stroke();
  const gray = (x: number, y: number, w: number, h: number) =>
    doc.rect(x, y, w, h).fill(GRAY).fillColor("black");

  /** Small bold label + value below it. */
  const field = (
    x: number,
    y: number,
    label: string,
    value: string,
    opts: { w?: number; big?: boolean } = {},
  ) => {
    const w = opts.w ?? 330;
    bold(opts.big ? 16 : 13).text(label, x, y, { width: w, lineBreak: false });
    reg(16).text(value, x, y + (opts.big ? 19 : 17), {
      width: w,
      height: 20,
      ellipsis: true,
    });
  };
  const sectionTitle = (y: number, title: string, h = 46) => {
    gray(LEFT, y, BOX_W, h);
    bold(17).text(title, COL[0], y + 2, { lineBreak: false });
  };

  // Page frame and header
  doc.lineWidth(2).rect(12, 12, 1389, 1976).stroke();
  gray(LEFT, 14, RIGHT - LEFT, 80);
  bold(54).fillColor("#4e9a6a").text("NFS", 26, 24, { lineBreak: false });
  bold(54).fillColor("#2e3a8c").text("e", 133, 38, { lineBreak: false });
  reg(18)
    .fillColor("#7c85a6")
    .text("Nota Fiscal de", 170, 44, { lineBreak: false })
    .text("Serviço eletrônica", 170, 62, { lineBreak: false });
  doc.fillColor("black");
  bold(21).text("DANFSe v2.0", 400, 32, { width: 610, align: "center" });
  bold(21).text("Documento Auxiliar da NFS-e", 400, 56, {
    width: 610,
    align: "center",
  });
  reg(19).text(
    `Município: ${inf.xLocEmi}${emitUf ? ` - ${emitUf}` : ""}`,
    1058,
    29,
    { lineBreak: false },
  );
  reg(13).text(`Ambiente Gerador: ${dash(inf.ambGer)}`, 1058, 50, {
    lineBreak: false,
  });
  reg(13).text(`Tipo de Ambiente: ${dash(dps.tpAmb)}`, 1058, 66, {
    lineBreak: false,
  });
  hline(95);

  // Identification
  field(COL[0], 108, "CHAVE DE ACESSO DA NFS-e", chave, { w: 700, big: true });
  const qr = await QRCode.toBuffer(
    `https://www.nfse.gov.br/ConsultaPublica/?tpc=1&chave=${chave}`,
    { margin: 1, width: 300 },
  );
  doc.image(qr, 1172, 108, { width: 100, height: 100 });
  reg(13).text(
    "A autenticidade desta NFS-e pode ser verificada pela leitura deste código QR ou pela consulta da chave de acesso no portal nacional da NFS-e",
    1058,
    220,
    { width: 330 },
  );
  const big = { big: true };
  field(COL[0], 156, "NÚMERO DA NFS-e", dash(inf.nNFSe), big);
  field(COL[1], 156, "COMPETÊNCIA DA NFS-e", date(dps.dCompet), big);
  field(
    COL[2],
    156,
    "DATA E HORA DA EMISSÃO DA NFS-e",
    dateTime(inf.dhProc),
    big,
  );
  field(COL[0], 203, "NÚMERO DA DPS", dash(dps.nDPS), big);
  field(COL[1], 203, "SÉRIE DA DPS", dash(dps.serie), big);
  field(COL[2], 203, "DATA E HORA DA EMISSÃO DA DPS", dateTime(dps.dhEmi), big);
  gray(LEFT, 249, BOX_W, 49);
  field(
    COL[0],
    251,
    "EMITENTE DA NFS-e",
    dps.tpEmit === "1" ? "Prestador" : "-",
    big,
  );
  field(
    COL[1],
    251,
    "SITUAÇÃO DA NFS-e",
    inf.cStat === "100" ? "NFS-e Gerada" : dash(inf.cStat),
    big,
  );
  field(COL[2], 251, "FINALIDADE", "-", big);
  hline(298);

  // Prestador
  sectionTitle(299, "PRESTADOR / FORNECEDOR");
  field(COL[1], 300, "CNPJ / CPF / NIF", cnpjCpf(emit.CNPJ ?? emit.CPF));
  field(COL[2], 300, "Indicador Municipal (Inscrição)", dash(dps.prest?.IM));
  field(COL[3], 300, "Telefone", phone(emit.fone));
  field(COL[0], 346, "Nome / Nome Empresarial", dash(emit.xNome), { w: 680 });
  field(COL[2], 346, "Município / Sigla UF", `${inf.xLocEmi} / ${emitUf}`);
  field(
    COL[3],
    346,
    "Código IBGE / CEP",
    `${ibge(emit.enderNac?.cMun)} / ${cep(emit.enderNac?.CEP)}`,
  );
  field(COL[0], 391, "Endereço", address(emit.enderNac), { w: 680 });
  field(COL[2], 391, "E-mail", dash(emit.email?.toLowerCase()));
  field(
    COL[0],
    436,
    "Simples Nacional na Data de Competência",
    SIMPLES[regTrib?.opSimpNac] ?? "-",
    { w: 335 },
  );
  field(
    COL[1],
    436,
    "Regime de Apuração Tributária pelo SN",
    regTrib?.opSimpNac === "3" ? (REG_AP[regTrib.regApTribSN] ?? "-") : "-",
    { w: 1000 },
  );
  hline(481);

  // Tomador
  sectionTitle(482, "TOMADOR / ADQUIRENTE");
  field(COL[1], 483, "CNPJ / CPF / NIF", cnpjCpf(toma.CNPJ ?? toma.CPF));
  field(COL[2], 483, "Indicador Municipal (Inscrição)", "-");
  field(COL[3], 483, "Telefone", phone(toma.fone));
  field(COL[0], 528, "Nome / Nome Empresarial", dash(toma.xNome), { w: 680 });
  field(COL[2], 528, "Município / Sigla UF", dash(tomaCityUf));
  field(
    COL[3],
    528,
    "Código IBGE / CEP",
    `${ibge(toma.end?.endNac?.cMun)} / ${cep(toma.end?.endNac?.CEP)}`,
  );
  field(COL[0], 573, "Endereço", address(toma.end), { w: 680 });
  field(COL[2], 573, "E-mail", dash(toma.email?.toLowerCase()));
  hline(617);

  reg(16).text(
    "DESTINATÁRIO DA OPERAÇÃO NÃO IDENTIFICADO NA NFS-e",
    LEFT,
    620,
    {
      width: RIGHT - LEFT,
      align: "center",
      lineBreak: false,
    },
  );
  hline(637);
  reg(16).text(
    "INTERMEDIÁRIO DA OPERAÇÃO NÃO IDENTIFICADO NA NFS-e",
    LEFT,
    640,
    { width: RIGHT - LEFT, align: "center", lineBreak: false },
  );
  hline(657);

  // Serviço
  const cTribNac = digits(serv.cServ?.cTribNac).replace(
    /^(.{2})(.{2})(.{2})$/,
    "$1.$2.$3",
  );
  const nbs = digits(serv.cServ?.cNBS).replace(
    /^(.)(.{4})(.{2})(.{2})$/,
    "$1.$2.$3.$4",
  );
  sectionTitle(658, "SERVIÇO PRESTADO");
  field(
    COL[1],
    659,
    "Código de Tributação Nacional/Municipal",
    `${cTribNac} / ${dash(serv.cServ?.cTribMun)}`,
  );
  field(COL[2], 659, "Código da NBS", dash(nbs));
  field(
    COL[3],
    659,
    "Local da Prestação / Sigla UF / País",
    `${dash(inf.xLocPrestacao)}${tomaUf ? ` / ${tomaUf}` : ""} / -`,
  );
  reg(16).text(dash(inf.xTribNac), COL[0], 704, {
    width: 1340,
    height: 20,
    ellipsis: true,
  });
  field(COL[0], 735, "Descrição do Serviço", dash(serv.cServ?.xDescServ), {
    w: 1340,
  });
  hline(778);

  // Tributação municipal
  sectionTitle(779, "TRIBUTAÇÃO MUNICIPAL (ISSQN)");
  field(
    COL[1],
    780,
    "Tipo de Tributação do ISSQN",
    TRIB_ISS[trib.tribMun?.tribISSQN] ?? "-",
  );
  field(
    COL[2],
    780,
    "Município / Sigla UF / País de Incidência do ISSQN",
    `${dash(inf.xLocIncid)} / ${emitUf} / -`,
    { w: 340 },
  );
  field(COL[0], 826, "BC ISSQN", "-");
  field(
    COL[1],
    826,
    "Alíquota Aplicada",
    pAliq ? `${Number(pAliq).toFixed(2).replace(".", ",")}%` : "-",
  );
  field(
    COL[2],
    826,
    "Retenção do ISSQN",
    RET_ISS[trib.tribMun?.tpRetISSQN] ?? "-",
  );
  field(COL[3], 826, "ISSQN Apurado", "-");
  hline(870);

  // Tributação federal
  sectionTitle(871, "TRIBUTAÇÃO FEDERAL (EXCETO CBS)");
  field(COL[1], 872, "IRRF", "-");
  field(COL[2], 872, "Contribuição Previdenciária - Retida", "-");
  field(COL[3], 872, "Contribuições Sociais - Retidas", "-");
  field(COL[0], 917, "PIS - Débito Apuração Própria", "-");
  field(COL[1], 917, "COFINS - Débito Apuração Própria", "-");
  field(
    COL[2],
    917,
    "Descrição Contrib. Sociais - Retidas",
    PIS_COFINS_RET[trib.tribFed?.piscofins?.tpRetPisCofins] ?? "-",
    { w: 340 },
  );
  hline(962);

  // IBS/CBS (not applicable yet)
  sectionTitle(963, "TRIBUTAÇÃO IBS/CBS");
  field(COL[1], 964, "CST / cClassTrib", "- / -");
  field(
    COL[2],
    964,
    "Indicador de Operação / Código IBGE Incidência / Município Incidência / Sigla UF",
    "- / - / - / -",
    { w: 680 },
  );
  field(COL[0], 1009, "Exclusões e Reduções da Base de Cálculo", "R$ 0,00");
  field(COL[1], 1009, "Base de Cálculo Após Exclusões e Reduções", "-");
  field(COL[2], 1009, "Red. Alíquota IBS / Red. Alíquota CBS", "- / -");
  field(COL[3], 1009, "Alíquota - IBS UF / IBS Mun", "- / -");
  field(COL[0], 1055, "Alíq. Efetiva Municipal - IBS", "-");
  field(COL[1], 1055, "Valor Apurado Municipal - IBS", "-");
  field(COL[2], 1055, "Alíq. Efetiva Estadual - IBS", "-");
  field(COL[3], 1055, "Valor Apurado Estadual - IBS", "-");
  field(COL[0], 1100, "Valor Total Apurado - IBS", "-");
  field(COL[1], 1100, "Alíquota - CBS", "-");
  field(COL[2], 1100, "Alíquota Efetiva - CBS", "-");
  field(COL[3], 1100, "Valor Total Apurado - CBS", "-");
  hline(1144);

  // Totals
  sectionTitle(1145, "VALOR TOTAL DA NFS-e");
  field(
    COL[1],
    1146,
    "VALOR DA OPERAÇÃO / SERVIÇO",
    brl(dps.valores?.vServPrest?.vServ),
  );
  field(COL[2], 1146, "Desconto Incondicionado", "-");
  field(COL[3], 1146, "Desconto Condicionado", "-");
  field(COL[0], 1190, "Total das Retenções (ISSQN / Federais)", "-");
  field(COL[1], 1190, "VALOR LÍQUIDO DA NFS-e", brl(inf.valores?.vLiq));
  field(COL[2], 1190, "Total do IBS/CBS", "R$ 0,00");
  gray(1050, 1190, RIGHT - 1050, 46);
  field(COL[3], 1190, "VALOR LÍQUIDO DA NFS-e + IBS/CBS", "R$ 0,00");
  hline(1237);

  bold(17).text("INFORMAÇÕES COMPLEMENTARES", COL[0], 1240, {
    lineBreak: false,
  });
  reg(16).text(
    "Totais aproximados dos Tributos cfe. Lei n° 12.741/2012: Federais: -; Estaduais: -; Municipais: -;",
    COL[0],
    1287,
    { width: 1340 },
  );

  // Footer box
  doc
    .lineWidth(3)
    .rect(LEFT, 1890, RIGHT - LEFT, 48)
    .stroke();
  doc.lineWidth(2);
  doc.moveTo(365, 1890).lineTo(365, 1938).stroke();
  doc.moveTo(708, 1890).lineTo(708, 1938).stroke();
  bold(14).text("DATA CIENTIFICAÇÃO:", 31, 1893, { lineBreak: false });
  bold(14).text("IDENTIFICAÇÃO E ASSINATURA", 373, 1893, { lineBreak: false });
  bold(14).text("N° NFS-e / CHAVE NFS-e", 717, 1893, { lineBreak: false });
  reg(16).text(`${dash(inf.nNFSe)} / ${chave}`, 717, 1910, {
    lineBreak: false,
  });

  doc.end();
  return done;
}
