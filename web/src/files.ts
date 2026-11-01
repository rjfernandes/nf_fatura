// Documents of a billing and exported files. The browser downloads them from
// the API; the desktop app opens them in the system viewer or asks where to
// save.
import { isDesktop, run } from "./desktop";

export type DocKind = "nfse_pdf" | "nfse_xml" | "slip_pdf";
export type DocRef = { kind: DocKind; id: string };

const paths: Record<DocKind, string> = {
  nfse_pdf: "nfse.pdf",
  nfse_xml: "nfse.xml",
  slip_pdf: "boleto.pdf",
};

export const documentUrl = (doc: DocRef) =>
  `/api/billings/${doc.id}/${paths[doc.kind]}`;

export const openDocument = (doc: DocRef, fileName: string) =>
  run<void>("document_open", { ...doc, fileName });

/** Resolves to false when the user cancels the save dialog. */
export const saveDocument = (doc: DocRef, fileName: string) =>
  run<boolean>("document_save", { ...doc, fileName });

export const statementExportUrl = (competence: string, format: "pdf" | "ofx") =>
  `/api/statements/export?competence=${competence}&format=${format}`;

export const exportStatement = (competence: string, format: "pdf" | "ofx") =>
  run<boolean>("statement_export", { competence, format });

/** Saves a text built on the screen (the statement CSV). */
export async function saveText(
  fileName: string,
  content: string,
  mime: string,
) {
  if (isDesktop) {
    await run<boolean>("text_save", { fileName, content });
    return;
  }
  const url = URL.createObjectURL(new Blob([content], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  a.click();
  URL.revokeObjectURL(url);
}
