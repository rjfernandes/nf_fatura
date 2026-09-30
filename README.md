# NF Fatura

Cadastro de clientes + emissão de NFS-e Nacional e boleto Banco Inter Empresas.

- `server/` – Fastify + Prisma (SQLite) + integrações (`integrations/nfse`, `integrations/inter`)
- `web/` – React + Vite + Tailwind

## Rodando
```bash
npm install
cp server/.env.example server/.env   # preencha as credenciais
npm run db:push -w server            # cria o banco SQLite
npm run dev                          # API :3333, web :5173
npm test
```

## Credenciais necessárias
- **NFS-e Nacional**: certificado A1 (.pfx) do prestador, CNPJ, código IBGE do município, `cTribNac` do serviço. Comece em `NFSE_ENV=homologacao` (produção restrita). O município precisa aderir ao Sistema Nacional.
- **Banco Inter**: aplicação com escopos `boleto-cobranca.read/write`, certificado (.crt) e chave (.key) gerados no portal, conta corrente. Comece em `INTER_ENV=sandbox`.

Coloque certificados em `server/certs/` (ignorado pelo git).

## Fluxo
Faturamento → seleciona clientes → para cada um: emite NFS-e e, se `hasBankSlip`, cria o boleto. Falhas são isoladas por cliente e podem ser reprocessadas (retoma do passo que falhou, sem reemitir a NFS-e).
