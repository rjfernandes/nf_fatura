use crate::error::{AppError, Result};
use crate::integrations::types::{ImportedNfse, TakerAddress};
use crate::util::cents_of;
use roxmltree::{Document, Node};

/// Child element by local name (the namespace prefix is ignored).
pub fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

/// Descendant following a path of local names.
pub fn path<'a, 'i>(node: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(node, |n, name| child(n, name))
}

/// Text of the descendant at the path, when it is there and not empty.
pub fn text(node: Node, names: &[&str]) -> Option<String> {
    path(node, names)
        .and_then(|n| n.text())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

pub fn invalid_nfse(e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("XML da NFS-e inválido: {e}"))
}

/// The `<infNFSe>` and its `<infDPS>` of an authorized NFS-e.
pub fn sections<'a, 'i>(doc: &'a Document<'i>) -> Result<(Node<'a, 'i>, Node<'a, 'i>)> {
    let root = doc.root_element();
    let inf = if root.tag_name().name() == "NFSe" {
        child(root, "infNFSe")
    } else {
        None
    }
    .ok_or_else(|| invalid_nfse("falta infNFSe"))?;
    let dps = path(inf, &["DPS", "infDPS"]).ok_or_else(|| invalid_nfse("falta infDPS"))?;
    Ok((inf, dps))
}

pub struct ParsedNfse {
    pub note: ImportedNfse,
    pub provider_tax_id: String,
}

/// Reads an authorized NFS-e XML. The emission date is kept as written (with
/// its offset) so the competence comes from the date the issuer saw, not from a
/// timezone conversion.
pub fn parse_nfse(xml: &str) -> Result<ParsedNfse> {
    let doc = Document::parse(xml).map_err(invalid_nfse)?;
    let (inf, dps) = sections(&doc)?;
    let value = text(inf, &["valores", "vLiq"])
        .or_else(|| text(dps, &["valores", "vServPrest", "vServ"]))
        .unwrap_or_else(|| "0".into());
    let issued_at = text(dps, &["dhEmi"])
        .or_else(|| text(inf, &["dhProc"]))
        .unwrap_or_default();
    let id = inf.attribute("Id").unwrap_or("");
    Ok(ParsedNfse {
        note: ImportedNfse {
            access_key: id.strip_prefix("NFS").unwrap_or(id).to_string(),
            number: text(inf, &["nNFSe"]).unwrap_or_default(),
            xml: xml.to_string(),
            competence: text(dps, &["dCompet"])
                .unwrap_or_else(|| issued_at.clone())
                .chars()
                .take(7)
                .collect(),
            issued_at,
            taker_tax_id: text(dps, &["toma", "CNPJ"])
                .or_else(|| text(dps, &["toma", "CPF"]))
                .unwrap_or_default(),
            taker_name: text(dps, &["toma", "xNome"]),
            taker_address: TakerAddress {
                address: text(dps, &["toma", "end", "xLgr"]),
                number: text(dps, &["toma", "end", "nro"]),
                complement: text(dps, &["toma", "end", "xCpl"]),
                neighborhood: text(dps, &["toma", "end", "xBairro"]),
                city_ibge_code: text(dps, &["toma", "end", "endNac", "cMun"]),
                zipcode: text(dps, &["toma", "end", "endNac", "CEP"]),
            },
            amount_cents: cents_of(value.parse().unwrap_or(0.0)),
        },
        provider_tax_id: text(inf, &["emit", "CNPJ"])
            .or_else(|| text(inf, &["emit", "CPF"]))
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse"><infNFSe Id="NFS123">
<nNFSe>42</nNFSe><dhProc>2026-10-01T00:10:00-03:00</dhProc>
<emit><CNPJ>11222333000181</CNPJ></emit><valores><vLiq>1234.50</vLiq></valores>
<DPS><infDPS><dCompet>2026-09-15</dCompet><dhEmi>2026-09-30T23:50:00-03:00</dhEmi>
<toma><CNPJ>99888777000166</CNPJ><xNome>Cliente</xNome></toma></infDPS></DPS>
</infNFSe></NFSe>"#;

    #[test]
    fn reads_key_number_taker_amount_and_keeps_the_emission_date_as_written() {
        let p = parse_nfse(XML).unwrap();
        assert_eq!(p.note.access_key, "123");
        assert_eq!(p.note.number, "42");
        assert_eq!(p.note.competence, "2026-09");
        assert_eq!(p.note.issued_at, "2026-09-30T23:50:00-03:00");
        assert_eq!(p.note.taker_tax_id, "99888777000166");
        assert_eq!(p.note.taker_name.as_deref(), Some("Cliente"));
        assert_eq!(p.note.amount_cents, 123450);
        assert_eq!(p.provider_tax_id, "11222333000181");
    }

    #[test]
    fn rejects_what_is_not_an_nfse() {
        assert!(parse_nfse("<a/>").is_err());
        assert!(parse_nfse("not xml").is_err());
    }
}
