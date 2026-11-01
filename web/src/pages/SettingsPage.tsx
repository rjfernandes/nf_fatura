import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState, type ReactNode } from "react";
import {
  settingsApi,
  type SecretsUpdate,
  type Settings,
  type SettingsView,
} from "../desktop";
import { confirmDialog } from "../dialogs";

const input =
  "mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";
const outlineButton =
  "rounded-md border border-slate-300 bg-white px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50";

type SecretKey = keyof SecretsUpdate;
const storedFlag: Record<SecretKey, keyof SettingsView> = {
  nfseCertPfxPassword: "hasNfseCertPfxPassword",
  interClientSecret: "hasInterClientSecret",
  camimToken: "hasCamimToken",
};

// The view carries more than the settings themselves.
function settingsOf(view: SettingsView): Settings {
  const {
    hasNfseCertPfxPassword: _p,
    hasInterClientSecret: _s,
    hasCamimToken: _t,
    databasePath: _d,
    ...settings
  } = view;
  return settings;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm">
      <h2 className="mb-3 font-semibold">{title}</h2>
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">{children}</div>
    </section>
  );
}

function Field({
  label,
  className = "",
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <label className={`block text-sm font-medium text-slate-700 ${className}`}>
      {label}
      {children}
    </label>
  );
}

export default function SettingsPage() {
  const qc = useQueryClient();
  const loaded = useQuery({
    queryKey: ["settings"],
    queryFn: settingsApi.get,
  });
  const [form, setForm] = useState<Settings>();
  const [secrets, setSecrets] = useState<SecretsUpdate>({});
  const [notice, setNotice] = useState("");

  // What was saved or imported becomes the form again.
  const adopt = (view: SettingsView) => {
    qc.setQueryData(["settings"], view);
    setForm(settingsOf(view));
    setSecrets({});
    // Integrations show whether they are configured.
    qc.invalidateQueries({ queryKey: ["integrations"] });
  };
  useEffect(() => {
    if (loaded.data && !form) {
      setForm(settingsOf(loaded.data));
    }
  }, [loaded.data, form]);

  const save = useMutation({
    mutationFn: () => settingsApi.save(form!, secrets),
    onSuccess: (view) => {
      adopt(view);
      setNotice("Configurações salvas.");
    },
  });
  const importEnv = useMutation({
    mutationFn: async () => {
      const path = await settingsApi.pickFile("Selecione o .env do servidor");
      return path ? settingsApi.importEnv(path) : null;
    },
    onSuccess: (view) => {
      if (view) {
        adopt(view);
        setNotice("Configurações importadas do .env.");
      }
    },
  });
  const importDb = useMutation({
    mutationFn: async () => {
      const path = await settingsApi.pickFile(
        "Selecione o banco de dados do servidor",
        ["db", "sqlite", "sqlite3"],
      );
      if (
        !path ||
        !(await confirmDialog(
          "Substituir os dados do aplicativo pelos do arquivo selecionado? O banco atual é mantido como backup.",
        ))
      ) {
        return null;
      }
      return settingsApi.importDatabase(path);
    },
    onSuccess: (result) => {
      if (result) {
        qc.invalidateQueries();
        setNotice(
          "Banco de dados importado." +
            (result.backup ? ` Backup do anterior: ${result.backup}` : ""),
        );
      }
    },
  });

  const view = loaded.data;
  if (loaded.error) {
    return (
      <p className="text-sm text-red-600">{(loaded.error as Error).message}</p>
    );
  }
  if (!form || !view) {
    return <p className="text-sm text-slate-500">Carregando...</p>;
  }

  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setNotice("");
    setForm({ ...form, [key]: value });
  };
  const text = (key: keyof Settings, props: { placeholder?: string } = {}) => (
    <input
      className={input}
      value={(form[key] as string | null) ?? ""}
      onChange={(e) => set(key, (e.target.value || null) as never)}
      {...props}
    />
  );
  // Fields the backend always expects a value for.
  const required = (key: keyof Settings) => (
    <input
      className={input}
      required
      value={form[key] as string}
      onChange={(e) => set(key, e.target.value as never)}
    />
  );
  const number = (key: keyof Settings, step: string) => (
    <input
      className={input}
      type="number"
      min={0}
      step={step}
      value={form[key] as number}
      onChange={(e) => set(key, Number(e.target.value) as never)}
    />
  );
  const select = (key: keyof Settings, options: [string, string][]) => (
    <select
      className={input}
      value={form[key] as string}
      onChange={(e) => set(key, e.target.value as never)}
    >
      {options.map(([value, label]) => (
        <option key={value} value={value}>
          {label}
        </option>
      ))}
    </select>
  );
  const file = (key: keyof Settings, title: string, extensions: string[]) => (
    <div className="mt-1 flex gap-2">
      <input
        className={`${input} mt-0`}
        readOnly
        value={(form[key] as string | null) ?? ""}
        placeholder="Nenhum arquivo"
        title={(form[key] as string | null) ?? undefined}
      />
      <button
        type="button"
        className={outlineButton}
        onClick={async () => {
          const path = await settingsApi.pickFile(title, extensions);
          if (path) {
            set(key, path as never);
          }
        }}
      >
        Escolher
      </button>
    </div>
  );
  const secret = (key: SecretKey) => {
    const stored = view[storedFlag[key]] as boolean;
    const removing = secrets[key] === "";
    return (
      <>
        <input
          className={input}
          type="password"
          autoComplete="off"
          value={secrets[key] ?? ""}
          placeholder={
            removing
              ? "Será removido ao salvar"
              : stored
                ? "Armazenado no Keychain (digite para trocar)"
                : "Não informado"
          }
          onChange={(e) => {
            setNotice("");
            const { [key]: _, ...rest } = secrets;
            setSecrets(
              e.target.value ? { ...rest, [key]: e.target.value } : rest,
            );
          }}
        />
        {stored && !removing && !secrets[key] && (
          <button
            type="button"
            className="mt-1 text-xs font-medium text-red-700 underline"
            onClick={() => setSecrets({ ...secrets, [key]: "" })}
          >
            Remover
          </button>
        )}
      </>
    );
  };

  const production =
    form.nfseEnv === "producao" || form.interEnv === "producao";
  const error = save.error ?? importEnv.error ?? importDb.error;

  return (
    <form
      className="space-y-6"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate();
      }}
    >
      <Section title="Empresa (prestador)">
        <Field label="Razão social">{required("companyName")}</Field>
        <Field label="CNPJ">{text("companyCnpj")}</Field>
        <Field label="Inscrição municipal">
          {text("companyMunicipalRegistration")}
        </Field>
        <Field label="Código IBGE do município">
          {text("companyCityIbge", { placeholder: "7 dígitos" })}
        </Field>
        <Field label="Telefone">{text("companyPhone")}</Field>
        <Field label="E-mail">{text("companyEmail")}</Field>
      </Section>

      <Section title="NFS-e Nacional">
        <Field label="Ambiente">
          {select("nfseEnv", [
            ["homologacao", "Homologação (produção restrita)"],
            ["producao", "Produção"],
          ])}
        </Field>
        <Field label="Certificado A1 (.pfx)" className="lg:col-span-2">
          {file("nfseCertPfxPath", "Selecione o certificado A1", [
            "pfx",
            "p12",
          ])}
        </Field>
        <Field label="Senha do certificado">
          {secret("nfseCertPfxPassword")}
        </Field>
        <Field label="Série da DPS">{required("nfseSeries")}</Field>
        <Field label="Último nDPS já usado na série">
          {number("nfseDpsOffset", "1")}
        </Field>
        <Field label="Código de tributação nacional (cTribNac)">
          {required("nfseServiceCode")}
        </Field>
        <Field label="Código de tributação municipal (cTribMun)">
          {required("nfseMunicipalServiceCode")}
        </Field>
        <Field label="Código NBS (cNBS)">{required("nfseNbsCode")}</Field>
        <Field
          label="Descrição do serviço"
          className="sm:col-span-2 lg:col-span-3"
        >
          {required("nfseServiceDescription")}
        </Field>
        <Field label="Simples Nacional">
          {select("nfseSimplesNacional", [
            ["1", "1 - Não optante"],
            ["2", "2 - MEI"],
            ["3", "3 - ME/EPP"],
          ])}
        </Field>
        <Field label="Regime de apuração (regApTribSN)">
          {select("nfseRegApTribSn", [
            ["1", "1 - Federais e municipal pelo SN"],
            ["2", "2 - Federais pelo SN, ISSQN pela NFS-e"],
            ["3", "3 - Federais e municipal pela NFS-e"],
          ])}
        </Field>
        <Field label="Total de tributos do SN (%)">
          {number("nfseTotTribSnPercent", "0.01")}
        </Field>
        <Field label="Alíquota do ISS (%), se não optante">
          {number("nfseIssRatePercent", "0.01")}
        </Field>
      </Section>

      <Section title="Banco Inter Empresas">
        <Field label="Ambiente">
          {select("interEnv", [
            ["sandbox", "Sandbox"],
            ["producao", "Produção"],
          ])}
        </Field>
        <Field label="Client ID">{text("interClientId")}</Field>
        <Field label="Client secret">{secret("interClientSecret")}</Field>
        <Field label="Certificado (.crt)">
          {file("interCertPath", "Selecione o certificado do Inter", [
            "crt",
            "pem",
          ])}
        </Field>
        <Field label="Chave (.key)">
          {file("interKeyPath", "Selecione a chave do Inter", ["key", "pem"])}
        </Field>
        <Field label="Conta corrente (com dígito)">
          {text("interAccount")}
        </Field>
        <Field label="Agência (exibida no extrato)">
          {required("interBranch")}
        </Field>
      </Section>

      <Section title="Camim (envio de NFS-e e boleto)">
        <Field label="URL da API" className="sm:col-span-2">
          {required("camimBaseUrl")}
        </Field>
        <Field label="Token">{secret("camimToken")}</Field>
      </Section>

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="submit"
          disabled={save.isPending}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          {save.isPending ? "Salvando..." : "Salvar"}
        </button>
        {production && (
          <span className="text-sm text-amber-700">
            Ambiente de produção: as NFS-e e os boletos emitidos são reais.
          </span>
        )}
        {notice && <span className="text-sm text-green-700">{notice}</span>}
        {error && (
          <span className="text-sm text-red-600">
            {(error as Error).message}
          </span>
        )}
      </div>

      <section className="rounded-lg border border-slate-200 bg-white p-4 shadow-sm">
        <h2 className="mb-1 font-semibold">Importar do servidor</h2>
        <p className="mb-3 text-sm text-slate-600">
          Traga as configurações do <code>.env</code> (com os certificados que
          ele aponta) e os dados de um banco SQLite já usado pelo servidor web.
        </p>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            className={outlineButton}
            disabled={importEnv.isPending}
            onClick={() => importEnv.mutate()}
          >
            {importEnv.isPending ? "Importando..." : "Importar .env"}
          </button>
          <button
            type="button"
            className={outlineButton}
            disabled={importDb.isPending}
            onClick={() => importDb.mutate()}
          >
            {importDb.isPending ? "Importando..." : "Importar banco de dados"}
          </button>
        </div>
        <p className="mt-3 break-all text-xs text-slate-500">
          Banco do aplicativo: {view.databasePath}
        </p>
        <p className="mt-1 text-xs text-slate-500">
          Depois de importar, emita por apenas um dos dois (web ou desktop): a
          numeração da DPS segue a mesma série.
        </p>
      </section>
    </form>
  );
}
