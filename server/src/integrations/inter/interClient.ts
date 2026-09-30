import type { SlipProvider, SlipRequest, SlipResult } from "../types.js";
import type { InterApi } from "./interApi.js";

const SCOPE = "boleto-cobranca.read boleto-cobranca.write";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export class InterSlipProvider implements SlipProvider {
  constructor(private inter: InterApi) {}

  private api<T>(method: "GET" | "POST", path: string, body?: unknown) {
    return this.inter.call<T>(SCOPE, method, path, body);
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
