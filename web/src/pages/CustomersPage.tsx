import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useForm } from "react-hook-form";
import { FiEdit2, FiTrash2 } from "react-icons/fi";
import {
  api,
  brl,
  formatCnpj,
  incompleteIntegration,
  type Customer,
  type CustomerInput,
  type IntegrationKey,
} from "../api";
import ActionButton from "../components/ActionButton";
import Table from "../components/Table";
import {
  centsToMasked,
  isValidCnpjInput,
  maskCep,
  maskCnpj,
  maskMoney,
  moneyToCents,
} from "../masks";

interface FormValues extends Omit<
  CustomerInput,
  "recurringValue" | "integration"
> {
  recurringValue: string; // reais, edited as text
  integration: string; // "" = none
}

const empty: FormValues = {
  name: "",
  cnpj: "",
  address: "",
  addressNumber: "",
  addressComplement: "",
  neighborhood: "",
  city: "",
  cityIbgeCode: "",
  state: "",
  zipcode: "",
  nickName: "",
  station: "",
  recurringValue: "",
  hasBankSlip: false,
  integration: "",
};

const input =
  "mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500";

function Field({
  label,
  className = "",
  children,
}: {
  label: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <label className={`block text-sm font-medium text-slate-700 ${className}`}>
      {label}
      {children}
    </label>
  );
}

function CustomerForm({
  initial,
  onDone,
}: {
  initial?: Customer;
  onDone: () => void;
}) {
  const qc = useQueryClient();
  const {
    register,
    handleSubmit,
    setValue,
    formState: { errors },
  } = useForm<FormValues>({
    defaultValues: initial
      ? {
          ...initial,
          cnpj: maskCnpj(initial.cnpj),
          zipcode: maskCep(initial.zipcode),
          addressComplement: initial.addressComplement ?? "",
          cityIbgeCode: initial.cityIbgeCode ?? "",
          nickName: initial.nickName ?? "",
          station: initial.station ?? "",
          integration: initial.integration ?? "",
          recurringValue: centsToMasked(initial.recurringValue),
        }
      : empty,
  });
  const [cepMsg, setCepMsg] = useState("");
  const [cnpjMsg, setCnpjMsg] = useState("");
  const lastCnpj = useRef("");
  const cnpjTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const integrations = useQuery({
    queryKey: ["integrations"],
    queryFn: api.integrations,
  });

  const save = useMutation({
    mutationFn: (v: FormValues) => {
      const payload: CustomerInput = {
        ...v,
        addressComplement: v.addressComplement || null,
        cityIbgeCode: v.cityIbgeCode || null,
        nickName: v.nickName || null,
        station: v.station || null,
        integration: (v.integration || null) as IntegrationKey | null,
        recurringValue: moneyToCents(v.recurringValue),
      };
      return initial
        ? api.updateCustomer(initial.id, payload)
        : api.createCustomer(payload);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["customers"] });
      onDone();
    },
  });

  async function lookupCnpj(masked: string) {
    lastCnpj.current = masked;
    setCnpjMsg("Buscando...");
    try {
      const res = await fetch(
        `https://brasilapi.com.br/api/cnpj/v1/${masked.replace(/\D/g, "")}`,
      );
      // The CNPJ was edited while waiting: this answer is stale.
      if (lastCnpj.current !== masked) {
        return;
      }
      if (res.status === 404) {
        return setCnpjMsg("não encontrado");
      }
      if (!res.ok) {
        return setCnpjMsg("falha ao consultar");
      }
      const d = await res.json();
      const name: string = d.razao_social ?? "";
      setValue("name", name);
      // First word of the company name, e.g. "BANCO DO BRASIL SA" -> "Banco".
      const first = name.split(/\s+/)[0] ?? "";
      setValue(
        "nickName",
        first.charAt(0).toUpperCase() + first.slice(1).toLowerCase(),
      );
      setValue("address", d.logradouro ?? "");
      setValue("addressNumber", d.numero ?? "");
      setValue("addressComplement", d.complemento ?? "");
      setValue("neighborhood", d.bairro ?? "");
      setValue("zipcode", maskCep(String(d.cep ?? "")));
      setValue("city", d.municipio ?? "");
      setValue("state", d.uf ?? "");
      setValue("cityIbgeCode", String(d.codigo_municipio_ibge ?? ""));
      setCnpjMsg("");
    } catch {
      if (lastCnpj.current === masked) {
        setCnpjMsg("falha ao consultar");
      }
    }
  }

  async function lookupCep(cep: string) {
    setCepMsg("Buscando CEP...");
    try {
      const d = await (
        await fetch(`https://viacep.com.br/ws/${cep}/json/`)
      ).json();
      if (d.erro) {
        return setCepMsg("CEP não encontrado");
      }
      setValue("address", d.logradouro ?? "");
      setValue("neighborhood", d.bairro ?? "");
      setValue("city", d.localidade ?? "");
      setValue("state", d.uf ?? "");
      setValue("cityIbgeCode", d.ibge ?? "");
      setCepMsg("");
    } catch {
      setCepMsg("Falha ao consultar CEP");
    }
  }

  return (
    <form
      onSubmit={handleSubmit((v) => save.mutate(v))}
      className="mb-6 rounded-lg border border-slate-200 bg-white p-4 shadow-sm"
    >
      <h2 className="mb-3 font-semibold">
        {initial ? "Editar cliente" : "Novo cliente"}
      </h2>
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-6">
        <Field
          label={`CNPJ ${cnpjMsg ? `— ${cnpjMsg}` : ""}`}
          className="sm:col-span-2"
        >
          <input
            className={`${input} ${errors.cnpj ? "border-red-500" : ""}`}
            required
            placeholder="00.000.000/0000-00"
            autoCapitalize="characters"
            {...register("cnpj", {
              validate: (v) => isValidCnpjInput(v) || "CNPJ inválido",
              onChange: (e) => {
                const m = maskCnpj(e.target.value);
                setValue("cnpj", m, { shouldValidate: !!errors.cnpj });
                setCnpjMsg("");
                clearTimeout(cnpjTimer.current);
                lastCnpj.current = "";
                // Only digits: the alphanumeric CNPJ is not in the API yet.
                if (
                  !initial &&
                  /^[\d./-]{18}$/.test(m) &&
                  isValidCnpjInput(m)
                ) {
                  cnpjTimer.current = setTimeout(() => lookupCnpj(m), 200);
                }
              },
            })}
          />
          {errors.cnpj && (
            <span className="mt-1 block text-xs font-normal text-red-600">
              {errors.cnpj.message}
            </span>
          )}
        </Field>
        <Field label="Nome" className="sm:col-span-4">
          <input className={input} required {...register("name")} />
        </Field>
        <Field label="Apelido" className="sm:col-span-2">
          <input className={input} {...register("nickName")} />
        </Field>
        <Field
          label={`CEP ${cepMsg ? `— ${cepMsg}` : ""}`}
          className="sm:col-span-2"
        >
          <input
            className={input}
            required
            placeholder="00000-000"
            {...register("zipcode", {
              onChange: (e) => {
                const m = maskCep(e.target.value);
                setValue("zipcode", m);
                if (m.length === 9) {
                  lookupCep(m.replace("-", ""));
                }
              },
            })}
          />
        </Field>
        <Field label="Endereço" className="sm:col-span-3">
          <input className={input} required {...register("address")} />
        </Field>
        <Field label="Número" className="sm:col-span-1">
          <input className={input} required {...register("addressNumber")} />
        </Field>
        <Field label="Complemento" className="sm:col-span-2">
          <input className={input} {...register("addressComplement")} />
        </Field>
        <Field label="Bairro" className="sm:col-span-2">
          <input className={input} required {...register("neighborhood")} />
        </Field>
        <Field label="Cidade" className="sm:col-span-2">
          <input className={input} required {...register("city")} />
        </Field>
        <Field label="Cód. IBGE" className="sm:col-span-1">
          <input
            className={input}
            maxLength={7}
            {...register("cityIbgeCode")}
          />
        </Field>
        <Field label="UF" className="sm:col-span-1">
          <input
            className={input}
            required
            maxLength={2}
            {...register("state")}
          />
        </Field>
        <Field label="Valor recorrente (R$)" className="sm:col-span-2">
          <input
            className={input}
            required
            inputMode="decimal"
            placeholder="0,00"
            {...register("recurringValue", {
              onChange: (e) =>
                setValue("recurringValue", maskMoney(e.target.value)),
            })}
          />
        </Field>
        <label className="flex items-center gap-2 pt-6 text-sm font-medium text-slate-700 sm:col-span-2">
          <input
            type="checkbox"
            className="h-4 w-4 rounded border-slate-300"
            {...register("hasBankSlip")}
          />{" "}
          Possui boleto bancário
        </label>
        <Field label="Integração de envio" className="sm:col-span-2">
          <select className={input} {...register("integration")}>
            <option value="">Nenhuma</option>
            {integrations.data?.map((i) => (
              <option key={i.key} value={i.key}>
                {i.label}
                {i.configured ? "" : " (não configurada)"}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Posto" className="sm:col-span-1">
          <input
            className={`${input} uppercase`}
            maxLength={1}
            placeholder="A"
            {...register("station", {
              onChange: (e) =>
                setValue(
                  "station",
                  e.target.value.toUpperCase().replace(/[^A-Z]/g, ""),
                ),
            })}
          />
        </Field>
      </div>
      {save.error && (
        <p className="mt-3 text-sm text-red-600">
          {(save.error as Error).message}
        </p>
      )}
      <div className="mt-4 flex gap-2">
        <button
          disabled={save.isPending}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          Salvar
        </button>
        <button
          type="button"
          onClick={onDone}
          className="rounded-md px-4 py-2 text-sm text-slate-600 hover:bg-slate-100"
        >
          Cancelar
        </button>
      </div>
    </form>
  );
}

export default function CustomersPage() {
  const qc = useQueryClient();
  const { data = [], isLoading } = useQuery({
    queryKey: ["customers"],
    queryFn: api.customers,
  });
  const [editing, setEditing] = useState<Customer | "new" | null>(null);
  const [q, setQ] = useState("");
  const integrations = useQuery({
    queryKey: ["integrations"],
    queryFn: api.integrations,
  });
  const remove = useMutation({
    mutationFn: api.deleteCustomer,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["customers"] }),
    onError: (e: Error) => alert(e.message),
  });
  const list = data.filter((c) =>
    (c.name + (c.nickName ?? "") + c.cnpj)
      .toLowerCase()
      .includes(q.toLowerCase()),
  );

  return (
    <div>
      <div className="mb-4 flex items-center gap-3">
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Buscar por nome, apelido ou CNPJ"
          className={`${input} mt-0 max-w-sm`}
        />
        <button
          onClick={() => setEditing("new")}
          className="ml-auto rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700"
        >
          Novo cliente
        </button>
      </div>
      {editing && (
        <CustomerForm
          key={editing === "new" ? "new" : editing.id}
          initial={editing === "new" ? undefined : editing}
          onDone={() => setEditing(null)}
        />
      )}
      <div className="overflow-x-auto rounded-lg border border-slate-200 bg-white shadow-sm">
        <Table
          headers={[
            {
              label: "Nome",
              field: (c) => <span className="font-medium">{c.name}</span>,
            },
            { label: "Apelido", field: (c) => c.nickName ?? "—" },
            { label: "Posto", field: (c) => c.station ?? "—" },
            { label: "CNPJ", field: (c) => formatCnpj(c.cnpj) },
            { label: "Cidade/UF", field: (c) => `${c.city}/${c.state}` },
            {
              label: "Valor",
              className: "text-right",
              field: (c) => brl(c.recurringValue),
            },
            { label: "Boleto", field: (c) => (c.hasBankSlip ? "Sim" : "Não") },
            {
              label: "Integração",
              field: (c) => {
                if (!c.integration) {
                  return "—";
                }
                const i = integrations.data?.find(
                  (i) => i.key === c.integration,
                );
                const incomplete = incompleteIntegration(i, c);
                return (
                  <>
                    {i?.label ?? c.integration}
                    {incomplete && (
                      <span
                        title={incomplete}
                        className="ml-2 rounded-full bg-amber-100 px-2 py-0.5 text-xs font-medium text-amber-800"
                      >
                        incompleta
                      </span>
                    )}
                  </>
                );
              },
            },
            {
              label: "",
              className: "text-right",
              field: (c) => (
                <div className="flex justify-end gap-2">
                  <ActionButton title="Editar" onClick={() => setEditing(c)}>
                    <FiEdit2 />
                  </ActionButton>
                  <ActionButton
                    variant="danger"
                    title="Excluir"
                    onClick={() =>
                      confirm(`Excluir ${c.name}?`) && remove.mutate(c.id)
                    }
                  >
                    <FiTrash2 />
                  </ActionButton>
                </div>
              ),
            },
          ]}
          rows={list}
          rowKey={(c) => c.id}
          emptyMessage={isLoading ? "Carregando..." : "Nenhum cliente."}
        />
      </div>
    </div>
  );
}
