# NF Fatura

Cadastro de clientes + emissão de NFS-e Nacional e boleto Banco Inter Empresas.

- `server/` – Fastify + Prisma (SQLite) + integrações (`integrations/nfse`, `integrations/inter`)
- `web/` – React + Vite + Tailwind
- `rust/` – app desktop para macOS (Tauri 2): as mesmas telas de `web/`, com a API reescrita em Rust

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

## App desktop (macOS)
```bash
npm install
npm run desktop:dev                  # janela nativa com hot reload (Vite na porta 5210)
npm run desktop:build                # gera rust/target/release/bundle (.app e .dmg)
cd rust && cargo test                # testes do backend
```
Requer Rust (rustup) e as Xcode Command Line Tools. Não usa o `server/`: o backend roda dentro do app.

- **Dados**: SQLite próprio em `~/Library/Application Support/br.com.mrf.nffatura/nf-fatura.db`, com o mesmo schema do Prisma.
- **Configurações**: na tela Configurações (no lugar do `.env`). Senha do certificado, client secret do Inter e token da Camim ficam no Keychain; os certificados escolhidos são copiados para a pasta do app. Uma instalação nova começa em homologação/sandbox.
- **Vindo do servidor web**: em Configurações → Importar do servidor, "Importar .env" traz as configurações e certificados e "Importar banco de dados" traz clientes e histórico (o banco anterior do app fica como backup ao lado). Depois disso, emita por apenas um dos dois: a numeração da DPS segue a mesma série.
- O servidor MCP continua sendo o de `server/`.
