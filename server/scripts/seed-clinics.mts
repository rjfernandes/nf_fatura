import { prisma } from "../src/db.js";

const RIO = { city: "Rio de Janeiro", cityIbgeCode: "3304557", state: "RJ" };

// recurringValue is not known for these companies: new records start at 0
// (cents) and existing ones keep whatever value they already have.
const clinics = [
  {
    name: "CENTRO MEDICO DOIS IRMAOS LTDA",
    cnpj: "72357999000131",
    address: "DE SANTA CRUZ",
    addressNumber: "598",
    neighborhood: "REALENGO",
    ...RIO,
    zipcode: "21710232",
  },
  {
    name: "POLICLINICA TRES IRMAOS LTDA",
    cnpj: "29470619000141",
    address: "CESARIO DE MELO",
    addressNumber: "3752",
    addressComplement: "SALAS 301/310, 401/412, 501/512 E 601",
    neighborhood: "CAMPO GRANDE",
    ...RIO,
    zipcode: "23050102",
  },
  {
    name: "CENTRO MEDICO FERNANDES LTDA",
    cnpj: "28593859000170",
    address: "ABILIO AUGUSTO TAVORA",
    addressNumber: "1111",
    addressComplement: "LOJAS 2009/2010",
    neighborhood: "DA LUZ",
    city: "Nova Iguaçu",
    cityIbgeCode: "3303500",
    state: "RJ",
    zipcode: "26260045",
  },
  {
    name: "CENTRO MEDICO ADELINO FERNANDES LTDA",
    cnpj: "33040053000195",
    address: "FRANCISCO REAL",
    addressNumber: "1783",
    addressComplement: "LOTE 3 PAL 8762",
    neighborhood: "BANGU",
    ...RIO,
    zipcode: "21810041",
  },
  {
    name: "CLINICA DE ASSISTENCIA MEDICA SAUDE PS LTDA",
    cnpj: "25247840000184",
    address: "PROF. ALFREDO GONCALVES FIGUEIRA",
    addressNumber: "100",
    addressComplement: "LOJAS 224 E 226",
    neighborhood: "CENTRO",
    city: "Nilópolis",
    cityIbgeCode: "3303203",
    state: "RJ",
    zipcode: "26525060",
  },
  {
    name: "CENTRO MEDICO CLARINDO BARROSO LOMBA LTDA",
    cnpj: "35978024000102",
    address: "AUGUSTO DE AZEVEDO SANTOS",
    addressNumber: "9",
    addressComplement: "LOJA A",
    neighborhood: "CAMPO GRANDE",
    ...RIO,
    zipcode: "23087040",
  },
  {
    name: "CENTRO MEDICO CGY CAMPO GRANDE LTDA",
    cnpj: "39396399000107",
    address: "DA CACHAMORRA",
    addressNumber: "1755",
    neighborhood: "CAMPO GRANDE",
    ...RIO,
    zipcode: "23040152",
  },
];

for (const c of clinics) {
  const r = await prisma.customer.upsert({
    where: { cnpj: c.cnpj },
    update: c,
    create: { ...c, recurringValue: 0, hasBankSlip: false },
  });
  console.log("ok", r.id, r.name);
}
