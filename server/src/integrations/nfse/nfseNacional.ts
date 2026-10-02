import { readFileSync } from "node:fs";
import { gunzipSync, gzipSync } from "node:zlib";
import { Agent, request } from "undici";
import type { Env } from "../../config/env.js";
import type {
  ImportedNfse,
  NfseProvider,
  NfseRequest,
  NfseResult,
} from "../types.js";
import { parseNfse } from "./parseNfse.js";
import { buildDpsXml, dpsId, type DpsConfig } from "./dps.js";
import { loadPfx, signDps } from "./sign.js";

const URLS = {
  homologacao: {
    sefin: "https://sefin.producaorestrita.nfse.gov.br/SefinNacional",
    adn: "https://adn.producaorestrita.nfse.gov.br",
  },
  producao: {
    sefin: "https://sefin.nfse.gov.br/SefinNacional",
    adn: "https://adn.nfse.gov.br",
  },
} as const;

export class NfseNacionalProvider implements NfseProvider {
  private ctx?: {
    cfg: DpsConfig;
    agent: Agent;
    material: ReturnType<typeof loadPfx>;
  };

  constructor(private env: Env) {}

  private init() {
    if (this.ctx) {
      return this.ctx;
    }
    const e = this.env;
    const required = {
      NFSE_CERT_PFX_PATH: e.NFSE_CERT_PFX_PATH,
      NFSE_CERT_PFX_PASSWORD: e.NFSE_CERT_PFX_PASSWORD,
      COMPANY_CNPJ: e.COMPANY_CNPJ,
      COMPANY_CITY_IBGE: e.COMPANY_CITY_IBGE,
    };
    const missing = Object.entries(required)
      .filter(([, v]) => !v)
      .map(([k]) => k);
    if (missing.length) {
      throw new Error(
        `NFS-e: variáveis não configuradas: ${missing.join(", ")}`,
      );
    }
    const pfx = readFileSync(e.NFSE_CERT_PFX_PATH!);
    const cfg: DpsConfig = {
      tpAmb: e.NFSE_ENV === "producao" ? 1 : 2,
      providerCnpj: e.COMPANY_CNPJ!.replace(/[^0-9a-zA-Z]/g, "").toUpperCase(),
      providerMunicipalRegistration: e.COMPANY_MUNICIPAL_REGISTRATION,
      providerCityIbge: e.COMPANY_CITY_IBGE!,
      providerPhone: e.COMPANY_PHONE?.replace(/\D/g, ""),
      providerEmail: e.COMPANY_EMAIL,
      serviceCode: e.NFSE_SERVICE_CODE,
      municipalServiceCode: e.NFSE_MUNICIPAL_SERVICE_CODE,
      nbsCode: e.NFSE_NBS_CODE,
      serviceDescription: e.NFSE_SERVICE_DESCRIPTION,
      opSimpNac: e.NFSE_SIMPLES_NACIONAL,
      regApTribSN: e.NFSE_REG_AP_TRIB_SN,
      issRatePercent: e.NFSE_ISS_RATE_PERCENT,
      totTribSNPercent: e.NFSE_TOT_TRIB_SN_PERCENT,
      series: e.NFSE_SERIES,
    };
    this.ctx = {
      cfg,
      agent: new Agent({
        connect: { pfx, passphrase: e.NFSE_CERT_PFX_PASSWORD },
      }),
      material: loadPfx(pfx, e.NFSE_CERT_PFX_PASSWORD!),
    };
    return this.ctx;
  }

  async issue(input: NfseRequest): Promise<NfseResult> {
    const { cfg, agent, material } = this.init();
    const urls = URLS[this.env.NFSE_ENV];
    const req = {
      ...input,
      dpsNumber: input.dpsNumber + this.env.NFSE_DPS_OFFSET,
    };
    // SEFIN rejects (E1229) a DPS without an explicit UTF-8 declaration. It is
    // added after signing; canonicalization ignores the declaration.
    const xml =
      '<?xml version="1.0" encoding="UTF-8"?>' +
      signDps(buildDpsXml(cfg, req), dpsId(cfg, req.dpsNumber), material);

    const res = await request(`${urls.sefin}/nfse`, {
      method: "POST",
      dispatcher: agent,
      headers: {
        "content-type": "application/json",
        accept: "application/json",
      },
      body: JSON.stringify({
        dpsXmlGZipB64: gzipSync(Buffer.from(xml, "utf8")).toString("base64"),
      }),
    });
    const text = await res.body.text();
    if (res.statusCode >= 300) {
      throw new Error(`NFS-e ${res.statusCode}: ${text}`);
    }

    const json = JSON.parse(text) as {
      nfseXmlGZipB64?: string;
      chaveAcesso?: string;
    };

    if (!json.chaveAcesso) {
      throw new Error(`NFS-e: resposta sem chave de acesso: ${text}`);
    }

    const nfseXml = json.nfseXmlGZipB64
      ? gunzipSync(Buffer.from(json.nfseXmlGZipB64, "base64")).toString("utf8")
      : undefined;
    const number = nfseXml?.match(/<nNFSe>(\d+)<\/nNFSe>/)?.[1];

    // The NFS-e already exists at this point; a DANFSe failure must not force
    // re-issuing.
    let pdf: Buffer | undefined;
    try {
      const d = await request(`${urls.adn}/danfse/${json.chaveAcesso}`, {
        method: "GET",
        dispatcher: agent,
      });
      const buf = Buffer.from(await d.body.arrayBuffer());
      if (d.statusCode === 200) {
        pdf = buf;
      }
    } catch {
      /* PDF stays unavailable; XML is stored */
    }
    return {
      number,
      accessKey: json.chaveAcesso,
      xml: nfseXml,
      pdf,
      raw: text,
    };
  }

  /**
   * Walks the ADN DF-e distribution by NSU and keeps the NFS-e issued by the
   * company (the feed also carries notes where it is the taker, and events).
   */
  async list(): Promise<ImportedNfse[]> {
    const { cfg, agent } = this.init();
    const urls = URLS[this.env.NFSE_ENV];
    const notes: ImportedNfse[] = [];
    let nsu = 0;
    for (;;) {
      const res = await request(
        `${urls.adn}/contribuintes/DFe/${nsu}?cnpjConsulta=${cfg.providerCnpj}&lote=true`,
        {
          method: "GET",
          dispatcher: agent,
          headers: { accept: "application/json" },
        },
      );
      const text = await res.body.text();
      // 404 is how the portal says there is nothing after this NSU.
      if (res.statusCode === 404) {
        break;
      }
      if (res.statusCode >= 300) {
        throw new Error(`NFS-e ${res.statusCode}: ${text.slice(0, 300)}`);
      }
      const json = JSON.parse(text) as {
        LoteDFe?: {
          NSU: number;
          TipoDocumento?: string;
          ArquivoXml?: string;
        }[];
      };
      const batch = json.LoteDFe ?? [];
      if (!batch.length) {
        break;
      }
      for (const doc of batch) {
        nsu = Math.max(nsu, doc.NSU);
        if (doc.TipoDocumento !== "NFSE" || !doc.ArquivoXml) {
          continue;
        }
        const xml = gunzipSync(Buffer.from(doc.ArquivoXml, "base64")).toString(
          "utf8",
        );
        const { providerTaxId, ...note } = parseNfse(xml);
        if (providerTaxId.toUpperCase() === cfg.providerCnpj) {
          notes.push(note);
        }
      }
    }
    return notes;
  }
}
