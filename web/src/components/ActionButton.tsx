import { useState, type ReactNode } from "react";
import { isDesktop } from "../desktop";
import { alertDialog } from "../dialogs";
import { documentUrl, openDocument, saveDocument, type DocRef } from "../files";

const variants = {
  primary: "border-indigo-200 text-indigo-700 hover:bg-indigo-50",
  danger: "border-red-200 text-red-700 hover:bg-red-50",
};
export const actionClass =
  "inline-flex items-center gap-1.5 whitespace-nowrap rounded-md border bg-white px-2.5 py-1 text-xs font-medium disabled:cursor-not-allowed disabled:opacity-50";

function Spinner() {
  return (
    <svg className="h-3 w-3 animate-spin" viewBox="0 0 24 24" fill="none">
      <circle
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="4"
        className="opacity-25"
      />
      <path
        d="M4 12a8 8 0 018-8"
        stroke="currentColor"
        strokeWidth="4"
        strokeLinecap="round"
      />
    </svg>
  );
}

/**
 * Small button that shows a spinner and disables itself while `onClick`
 * (sync or async) is running. Errors are expected to be handled by the caller.
 */
export default function ActionButton({
  onClick,
  variant = "primary",
  disabled = false,
  title,
  children,
}: {
  onClick: () => unknown;
  variant?: keyof typeof variants;
  disabled?: boolean;
  title?: string;
  children: ReactNode;
}) {
  const [busy, setBusy] = useState(false);

  async function handle() {
    setBusy(true);
    try {
      await onClick();
    } catch {
      // surfaced by the caller (mutation onError)
    } finally {
      setBusy(false);
    }
  }

  return (
    <button
      type="button"
      onClick={handle}
      disabled={busy || disabled}
      title={title}
      className={`${actionClass} ${variants[variant]}`}
    >
      {busy && <Spinner />}
      {children}
    </button>
  );
}

/**
 * Opens a document of a billing, or downloads it under `fileName` when
 * `download` is set.
 */
export function ActionLink({
  doc,
  fileName,
  download = false,
  children,
}: {
  doc: DocRef;
  fileName: string;
  download?: boolean;
  children: ReactNode;
}) {
  if (isDesktop) {
    return (
      <ActionButton
        onClick={() =>
          (download ? saveDocument : openDocument)(doc, fileName).catch((e) =>
            alertDialog((e as Error).message),
          )
        }
      >
        {children}
      </ActionButton>
    );
  }
  return (
    <a
      href={documentUrl(doc)}
      download={download ? fileName : undefined}
      target={download ? undefined : "_blank"}
      className={`${actionClass} ${variants.primary}`}
    >
      {children}
    </a>
  );
}
