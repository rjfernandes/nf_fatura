import type { Env } from "../../config/env.js";
import type {
  DeliveryCustomer,
  DeliveryField,
  DeliveryProvider,
  DeliveryRequest,
} from "../types.js";

type CamimEnv = Pick<Env, "CAMIM_BASE_URL" | "CAMIM_TOKEN">;

/**
 * Camim PJ portal: the boleto of the month goes together with its NFS-e. A
 * single token identifies us; the customer's posto tells which clinic it is.
 */
export class CamimDeliveryProvider implements DeliveryProvider {
  label = "Camim";
  requiredFields: DeliveryField[] = ["station"];

  constructor(private env: CamimEnv) {}

  get configured() {
    return !!this.env.CAMIM_TOKEN;
  }

  private get url() {
    return `${this.env.CAMIM_BASE_URL.replace(/\/+$/, "")}/boletos/`;
  }

  private headers() {
    if (!this.env.CAMIM_TOKEN) {
      throw new Error("Camim: CAMIM_TOKEN não configurado");
    }
    return { authorization: `Bearer ${this.env.CAMIM_TOKEN}` };
  }

  private async call(url: string, init: RequestInit) {
    const res = await fetch(url, init);
    const raw = await res.text();
    if (!res.ok) {
      throw new Error(`Camim: HTTP ${res.status} ${raw.slice(0, 300)}`);
    }
    return raw;
  }

  /**
   * The month's query lists the boletos already there, one per posto. The pair
   * counts as sent when there is one for the customer's posto (posto_letra).
   */
  async alreadySent(customer: DeliveryCustomer, competence: string) {
    const raw = await this.call(
      `${this.url}?competencia=${encodeURIComponent(competence)}`,
      { headers: this.headers() },
    );
    const station = customer.station?.toUpperCase();
    const item = station
      ? boletosOf(raw).find(
          (i) => i.posto_letra?.trim().toUpperCase() === station,
        )
      : undefined;
    if (!item) {
      return { sent: false, raw };
    }
    const detail = [
      `id ${item.id}`,
      item.situacao ?? item.status,
      item.tem_nota_fiscal === false && "sem nota fiscal",
    ]
      .filter(Boolean)
      .join(", ");
    return { sent: true, raw, detail };
  }

  async send(req: DeliveryRequest) {
    if (!req.customer.station) {
      throw new Error("Camim: cliente sem posto");
    }
    const form = new FormData();
    form.append("competencia", req.competence);
    form.append("posto", req.customer.station);
    form.append(
      "arquivo",
      new Blob([new Uint8Array(req.slipPdf)], { type: "application/pdf" }),
      `boleto-${req.competence}.pdf`,
    );
    form.append(
      "nota_fiscal",
      new Blob([new Uint8Array(req.nfsePdf)], { type: "application/pdf" }),
      `nfse-${req.competence}.pdf`,
    );
    const raw = await this.call(this.url, {
      method: "POST",
      headers: this.headers(),
      body: form,
    });
    return { raw };
  }
}

interface CamimBoleto {
  id: number;
  competencia: string;
  posto: string | null; // name, e.g. "Y Campo Grande"
  posto_letra: string | null; // the letter we send as posto
  status: string; // e.g. APROVADO, FIN_RECEBIDO
  situacao: string | null; // human-readable status
  tem_nota_fiscal: boolean;
}

/** Boletos of the query response: { competencia, boletos: [...] }. */
function boletosOf(raw: string): CamimBoleto[] {
  try {
    const data = JSON.parse(raw);
    return Array.isArray(data?.boletos) ? data.boletos : [];
  } catch {
    return [];
  }
}
