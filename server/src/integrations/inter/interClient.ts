import type {
  ImportedSlip,
  SlipProvider,
  SlipRequest,
  SlipResult,
} from "../types.js";
import type { InterApi } from "./interApi.js";

const SCOPE = "boleto-cobranca.read boleto-cobranca.write";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export class InterSlipProvider implements SlipProvider {
  constructor(private inter: InterApi) {}

  private api<T>(method: "GET" | "POST", path: string, body?: unknown) {
    return this.inter.call<T>(SCOPE, method, path, body);
  }

  async list(from: string, to: string, known: Set<string>) {
    const slips: ImportedSlip[] = [];
    let pages = 1;
    for (let page = 0; page < pages; page++) {
      const q = new URLSearchParams({
        dataInicial: from,
        dataFinal: to,
        filtrarDataPor: "VENCIMENTO",
        paginaAtual: String(page),
      });
      const res = await this.api<{ totalPaginas?: number; cobrancas?: any[] }>(
        "GET",
        `/cobranca/v3/cobrancas?${q}`,
      );
      pages = res.totalPaginas ?? 1;
      for (const { cobranca: c, boleto: b } of res.cobrancas ?? []) {
        if (c.situacao === "CANCELADO") {
          continue;
        }
        const p = c.pagador ?? {};
        let pdf: Buffer | undefined;
        if (!known.has(c.codigoSolicitacao)) {
          try {
            const r = await this.api<{ pdf: string }>(
              "GET",
              `/cobranca/v3/cobrancas/${c.codigoSolicitacao}/pdf`,
            );
            pdf = Buffer.from(r.pdf, "base64");
          } catch {
            /* the boleto is imported without PDF */
          }
        }
        slips.push({
          codigoSolicitacao: c.codigoSolicitacao,
          nossoNumero: b?.nossoNumero,
          linhaDigitavel: b?.linhaDigitavel,
          barcode: b?.codigoBarras,
          status: c.situacao,
          dueDate: c.dataVencimento,
          amountCents: Math.round(Number(c.valorNominal) * 100),
          pdf,
          payer: {
            taxId: String(p.cpfCnpj ?? ""),
            name: p.nome ?? "",
            address: p.endereco,
            number: p.numero,
            complement: p.complemento || undefined,
            neighborhood: p.bairro,
            city: p.cidade,
            state: p.uf,
            zipcode: p.cep,
          },
        });
      }
    }
    return slips;
  }

  async cancel(codigoSolicitacao: string, reason: string): Promise<void> {
    await this.api(
      "POST",
      `/cobranca/v3/cobrancas/${codigoSolicitacao}/cancelar`,
      { motivoCancelamento: reason },
    );
  }

  async create(req: SlipRequest): Promise<SlipResult> {
    const c = req.customer;
    const { codigoSolicitacao } = await this.api<{ codigoSolicitacao: string }>(
      "POST",
      "/cobranca/v3/cobrancas",
      {
        seuNumero: req.billingId.slice(-15),
        valorNominal: req.amountCents / 100,
        dataVencimento: req.dueDate,
        numDiasAgenda: 30,
        pagador: {
          cpfCnpj: c.cnpj,
          tipoPessoa: "JURIDICA",
          nome: c.name,
          endereco: c.address,
          numero: c.addressNumber,
          complemento: c.addressComplement ?? "",
          bairro: c.neighborhood,
          cidade: c.city,
          uf: c.state,
          cep: c.zipcode,
        },
      },
    );

    // The boleto is registered asynchronously; poll until it has a barcode.
    let detail: any;
    for (let i = 0; i < 10; i++) {
      detail = await this.api(
        "GET",
        `/cobranca/v3/cobrancas/${codigoSolicitacao}`,
      );
      if (detail?.boleto?.linhaDigitavel) {
        break;
      }
      await sleep(1500);
    }
    if (!detail?.boleto?.linhaDigitavel) {
      throw new Error(
        `Banco Inter: boleto ${codigoSolicitacao} ainda em processamento; tente novamente`,
      );
    }

    const pdf = await this.api<{ pdf: string }>(
      "GET",
      `/cobranca/v3/cobrancas/${codigoSolicitacao}/pdf`,
    );
    return {
      codigoSolicitacao,
      nossoNumero: detail.boleto.nossoNumero,
      linhaDigitavel: detail.boleto.linhaDigitavel,
      barcode: detail.boleto.codigoBarras,
      status: detail.cobranca?.situacao,
      pdf: Buffer.from(pdf.pdf, "base64"),
    };
  }
}
