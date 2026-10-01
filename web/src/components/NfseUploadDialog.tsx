import { useEffect, useRef, useState } from "react";
import type { Billing } from "../api";

const MAX_BYTES = 10 * 1024 * 1024;
const input =
  "mt-1 block w-full rounded-md border border-slate-300 px-3 py-2 text-sm";

type Props = {
  /** Billing receiving the NFS-e; the dialog is open while set. */
  billing: Billing | null;
  pending: boolean;
  error: Error | null;
  onSubmit: (file: File, number: string) => void;
  onClose: () => void;
};

export default function NfseUploadDialog({
  billing,
  pending,
  error,
  onSubmit,
  onClose,
}: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [file, setFile] = useState<File | null>(null);
  const [number, setNumber] = useState("");
  const [invalid, setInvalid] = useState<string | null>(null);

  useEffect(() => {
    if (billing) {
      setFile(null);
      setNumber(billing.invoice?.number ?? "");
      setInvalid(null);
      ref.current?.showModal();
    } else {
      ref.current?.close();
    }
  }, [billing]);

  const pick = (f: File | null) => {
    setFile(f);
    setInvalid(
      !f
        ? null
        : f.type !== "application/pdf"
          ? "Selecione um arquivo PDF."
          : f.size > MAX_BYTES
            ? "O PDF deve ter no máximo 10 MB."
            : null,
    );
  };
  // The API invoice keeps its own number; only manual ones take one.
  const askNumber = billing?.invoice?.source !== "API";

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      className="m-auto w-full max-w-md rounded-lg p-0 shadow-xl backdrop:bg-slate-900/40"
    >
      {billing && (
        <form
          method="dialog"
          className="space-y-4 p-5"
          onSubmit={(e) => {
            e.preventDefault();
            if (file && !invalid) {
              onSubmit(file, number.trim());
            }
          }}
        >
          <div>
            <h3 className="font-semibold">Anexar NFS-e</h3>
            <p className="text-sm text-slate-600">
              {billing.customer.name} · {billing.competence}
            </p>
          </div>
          <label className="block text-sm font-medium text-slate-700">
            PDF da nota
            <input
              type="file"
              accept="application/pdf"
              required
              onChange={(e) => pick(e.target.files?.[0] ?? null)}
              className={input}
            />
          </label>
          {askNumber && (
            <label className="block text-sm font-medium text-slate-700">
              Número da NFS-e (opcional)
              <input
                value={number}
                maxLength={20}
                onChange={(e) => setNumber(e.target.value)}
                className={input}
              />
            </label>
          )}
          {(invalid || error) && (
            <p className="text-sm text-red-600">{invalid ?? error?.message}</p>
          )}
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md px-4 py-2 text-sm font-medium text-slate-600 hover:bg-slate-100"
            >
              Cancelar
            </button>
            <button
              type="submit"
              disabled={!file || !!invalid || pending}
              className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
            >
              {pending ? "Enviando..." : "Anexar"}
            </button>
          </div>
        </form>
      )}
    </dialog>
  );
}
