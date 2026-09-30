import { prisma } from "../src/db.js";

const customers = [
  {
    name: "CLINICA DE ASSISTENCIA MEDICA ANCHIETA LTDA",
    cnpj: "27110113000104",
    address: "MARECHAL ALENCASTRO",
    addressNumber: "3759",
    neighborhood: "ANCHIETA",
    city: "Rio de Janeiro",
    cityIbgeCode: "3304557",
    state: "RJ",
    zipcode: "21625000",
    recurringValue: 2425500, // R$ 24.255,00 em centavos
    hasBankSlip: false,
  },
  {
    name: "AGUA MINERAL CASCATAI LTDA",
    cnpj: "01374419000176",
    address: "JAIME FONSECA NOGUEIRA",
    addressNumber: "SN",
    addressComplement: "CASCATA ACU",
    neighborhood: "GUAPIACU",
    city: "Cachoeiras de Macacu",
    cityIbgeCode: "3300803",
    state: "RJ",
    zipcode: "28696300",
    recurringValue: 137785, // R$ 1.377,85 em centavos
    hasBankSlip: false,
  },
];

for (const c of customers) {
  const r = await prisma.customer.upsert({
    where: { cnpj: c.cnpj },
    update: c,
    create: c,
  });
  console.log("ok", r.id, r.name);
}
