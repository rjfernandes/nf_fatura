import { afterEach, describe, expect, it, vi } from "vitest";
import { CamimDeliveryProvider } from "./camimClient.js";

const env = {
  CAMIM_BASE_URL: "https://pj.camim.com.br/api/",
  CAMIM_TOKEN: "t",
};
const customer = {
  name: "C",
  cnpj: "11222333000181",
  address: "Rua A",
  addressNumber: "1",
  neighborhood: "Centro",
  city: "Rio",
  state: "RJ",
  zipcode: "20000000",
  station: "G",
};

function stubFetch(body: string, status = 200) {
  const fetch = vi.fn(
    async (_url: string, _init?: RequestInit) => new Response(body, { status }),
  );
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("CamimDeliveryProvider", () => {
  it("posts competence, boleto and NFS-e as multipart with the bearer token", async () => {
    const fetch = stubFetch('{"id":1}', 201);
    const r = await new CamimDeliveryProvider(env).send({
      customer,
      competence: "2026-09",
      nfsePdf: Buffer.from("nf"),
      slipPdf: Buffer.from("boleto"),
    });
    expect(r.raw).toBe('{"id":1}');
    const [url, init] = fetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe("https://pj.camim.com.br/api/boletos/");
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ authorization: "Bearer t" });
    const form = init.body as FormData;
    expect(form.get("competencia")).toBe("2026-09");
    expect(form.get("posto")).toBe("G");
    expect(await (form.get("arquivo") as File).text()).toBe("boleto");
    expect(await (form.get("nota_fiscal") as File).text()).toBe("nf");
  });

  it("posts only the document that was chosen", async () => {
    const fetch = stubFetch("{}", 201);
    const p = new CamimDeliveryProvider(env);
    await p.send({
      customer,
      competence: "2026-09",
      nfsePdf: Buffer.from("nf"),
    });
    let form = (fetch.mock.calls[0][1] as RequestInit).body as FormData;
    expect(form.has("arquivo")).toBe(false);
    expect(form.has("nota_fiscal")).toBe(true);
    await p.send({
      customer,
      competence: "2026-09",
      slipPdf: Buffer.from("boleto"),
    });
    form = (fetch.mock.calls[1][1] as RequestInit).body as FormData;
    expect(form.has("arquivo")).toBe(true);
    expect(form.has("nota_fiscal")).toBe(false);
    await expect(p.send({ customer, competence: "2026-09" })).rejects.toThrow(
      "nada para enviar",
    );
  });

  it("an NFS-e alone is not sent yet when the boleto there has no nota", async () => {
    const item = (nota: boolean) =>
      JSON.stringify({
        boletos: [{ id: 1, posto_letra: "G", tem_nota_fiscal: nota }],
      });
    const p = new CamimDeliveryProvider(env);
    stubFetch(item(false));
    expect((await p.alreadySent(customer, "2026-09", "NFSE")).sent).toBe(false);
    expect((await p.alreadySent(customer, "2026-09", "SLIP")).sent).toBe(true);
    stubFetch(item(true));
    expect((await p.alreadySent(customer, "2026-09", "NFSE")).sent).toBe(true);
  });

  it("refuses to send without the customer's posto", async () => {
    const fetch = stubFetch("{}");
    await expect(
      new CamimDeliveryProvider(env).send({
        customer: { ...customer, station: null },
        competence: "2026-09",
        nfsePdf: Buffer.from("nf"),
        slipPdf: Buffer.from("boleto"),
      }),
    ).rejects.toThrow("sem posto");
    expect(fetch).not.toHaveBeenCalled();
  });

  it("queries by competence and only counts the customer's posto as sent", async () => {
    const boleto = (id: number, letra: string | null, nota = true) => ({
      id,
      competencia: "2026-09",
      posto: letra && `${letra} Campo Grande`,
      posto_letra: letra,
      status: "APROVADO",
      situacao: "Valor confere — enviado p/ pagamento",
      valor_esperado: "1712.50",
      tem_nota_fiscal: nota,
    });
    const fetch = stubFetch(
      JSON.stringify({
        competencia: "2026-09",
        boletos: [boleto(38, null), boleto(97, "A"), boleto(98, "g", false)],
      }),
    );
    const p = new CamimDeliveryProvider(env);
    expect(await p.alreadySent(customer, "2026-09")).toMatchObject({
      sent: true,
      detail: "id 98, Valor confere — enviado p/ pagamento, sem nota fiscal",
    });
    expect(fetch.mock.calls[0][0]).toBe(
      "https://pj.camim.com.br/api/boletos/?competencia=2026-09",
    );
    // other postos (or none) don't count, nor does the name alone
    stubFetch(
      JSON.stringify({
        boletos: [
          boleto(38, null),
          boleto(99, "X"),
          { ...boleto(100, null), posto: "G" },
        ],
      }),
    );
    expect((await p.alreadySent(customer, "2026-09")).sent).toBe(false);
    stubFetch('{"competencia":"2026-09","boletos":[]}');
    expect((await p.alreadySent(customer, "2026-09")).sent).toBe(false);
  });

  it("fails on HTTP errors and without a token", async () => {
    stubFetch("nope", 401);
    await expect(
      new CamimDeliveryProvider(env).alreadySent(customer, "2026-09"),
    ).rejects.toThrow("HTTP 401");
    const p = new CamimDeliveryProvider({ ...env, CAMIM_TOKEN: undefined });
    expect(p.configured).toBe(false);
    await expect(p.alreadySent(customer, "2026-09")).rejects.toThrow(
      "CAMIM_TOKEN",
    );
  });
});
