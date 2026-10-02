const UF: Record<string, string> = {
  "11": "RO",
  "12": "AC",
  "13": "AM",
  "14": "RR",
  "15": "PA",
  "16": "AP",
  "17": "TO",
  "21": "MA",
  "22": "PI",
  "23": "CE",
  "24": "RN",
  "25": "PB",
  "26": "PE",
  "27": "AL",
  "28": "SE",
  "29": "BA",
  "31": "MG",
  "32": "ES",
  "33": "RJ",
  "35": "SP",
  "41": "PR",
  "42": "SC",
  "43": "RS",
  "50": "MS",
  "51": "MT",
  "52": "GO",
  "53": "DF",
};

/**
 * City name and state for an IBGE municipality code. The state comes from the
 * code itself; the name from IBGE's public API, "-" when it can't be reached.
 */
export async function cityOf(code: string) {
  const state = UF[code.slice(0, 2)] ?? "-";
  try {
    const res = await fetch(
      `https://servicodados.ibge.gov.br/api/v1/localidades/municipios/${code}`,
      { signal: AbortSignal.timeout(5000) },
    );
    const data = (await res.json()) as { nome?: string };
    return { city: data.nome ?? "-", state };
  } catch {
    return { city: "-", state };
  }
}
