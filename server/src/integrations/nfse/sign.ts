import forge from "node-forge";
import { SignedXml } from "xml-crypto";

export interface PfxMaterial {
  privateKeyPem: string;
  certPem: string;
}

export function loadPfx(pfx: Buffer, password: string): PfxMaterial {
  const p12 = forge.pkcs12.pkcs12FromAsn1(
    forge.asn1.fromDer(pfx.toString("binary")),
    password,
  );
  const keyBag =
    p12.getBags({ bagType: forge.pki.oids.pkcs8ShroudedKeyBag })[
      forge.pki.oids.pkcs8ShroudedKeyBag
    ]?.[0] ??
    p12.getBags({ bagType: forge.pki.oids.keyBag })[forge.pki.oids.keyBag]?.[0];
  const certBag = p12.getBags({ bagType: forge.pki.oids.certBag })[
    forge.pki.oids.certBag
  ]?.[0];
  if (!keyBag?.key || !certBag?.cert) {
    throw new Error("PFX inválido: chave ou certificado não encontrados");
  }
  return {
    privateKeyPem: forge.pki.privateKeyToPem(keyBag.key),
    certPem: forge.pki.certificateToPem(certBag.cert),
  };
}

const ALG = {
  sha256: {
    sig: "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256",
    digest: "http://www.w3.org/2001/04/xmlenc#sha256",
  },
  sha1: {
    sig: "http://www.w3.org/2000/09/xmldsig#rsa-sha1",
    digest: "http://www.w3.org/2000/09/xmldsig#sha1",
  },
} as const;

/** Enveloped XMLDSig over infDPS (referenced by its Id), appended to <DPS>. */
export function signDps(
  xml: string,
  id: string,
  m: PfxMaterial,
  alg: keyof typeof ALG = "sha256",
): string {
  const sig = new SignedXml({
    privateKey: m.privateKeyPem,
    publicCert: m.certPem,
    signatureAlgorithm: ALG[alg].sig,
    canonicalizationAlgorithm:
      "http://www.w3.org/TR/2001/REC-xml-c14n-20010315",
  });
  sig.addReference({
    xpath: `//*[@Id='${id}']`,
    transforms: [
      "http://www.w3.org/2000/09/xmldsig#enveloped-signature",
      "http://www.w3.org/TR/2001/REC-xml-c14n-20010315",
    ],
    digestAlgorithm: ALG[alg].digest,
  });
  sig.computeSignature(xml, {
    location: { reference: "/*", action: "append" },
  });
  return sig.getSignedXml();
}
