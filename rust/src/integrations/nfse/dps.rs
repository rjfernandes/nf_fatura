use super::xml::El;
use crate::error::{AppError, Result};
use crate::integrations::types::NfseRequest;
use crate::util::period::month_label;
use chrono::{DateTime, Duration, Utc};

pub const NFSE_NS: &str = "http://www.sped.fazenda.gov.br/nfse";

#[derive(Debug, Clone, Default)]
pub struct DpsConfig {
    pub tp_amb: u8, // 1 produção, 2 homologação
    pub provider_cnpj: String,
    pub provider_municipal_registration: Option<String>,
    pub provider_city_ibge: String,
    pub provider_phone: Option<String>,
    pub provider_email: Option<String>,
    pub service_code: String,                   // cTribNac
    pub municipal_service_code: Option<String>, // cTribMun
    pub nbs_code: Option<String>,               // cNBS
    pub service_description: String, // {mes} / {mes_anterior} become the month name
    pub op_simp_nac: String,            // 1 | 2 | 3
    pub reg_ap_trib_sn: Option<String>, // only sent when opSimpNac = 3 (ME/EPP)
    pub iss_rate_percent: f64,
    pub tot_trib_sn_percent: Option<f64>, // pTotTribSN, only sent when opSimpNac ≠ 1
    pub series: Option<String>,
    // NT 009 v1.01 layout (finNFSe, cNBS before xDescServ): required by produção
    // restrita since 2026-10-20; produção still takes the previous one.
    pub layout_nt009: bool,
    pub now: Option<DateTime<Utc>>,
}

fn money(cents: i64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

fn filled(v: &Option<String>) -> Option<&str> {
    v.as_deref().filter(|s| !s.is_empty())
}

/// DPS Id = "DPS" + cLocEmi(7) + tpInsc(1) + CNPJ(14) + serie(5) + nDPS(15) = 45
/// chars.
pub fn dps_id(cfg: &DpsConfig, n_dps: i64) -> String {
    format!(
        "DPS{}2{}{:0>5}{:015}",
        cfg.provider_city_ibge,
        cfg.provider_cnpj,
        cfg.series.as_deref().unwrap_or("1"),
        n_dps
    )
}

/// Fills {mes} with the competence month and {mes_anterior} with the month
/// before it ("Outubro de 2026").
pub fn service_description(template: &str, competence: &str) -> String {
    template
        .replace("{mes_anterior}", &month_label(competence, -1))
        .replace("{mes}", &month_label(competence, 0))
}

/// Brasília-time ISO timestamp with -03:00 offset.
fn brt_iso(d: DateTime<Utc>) -> String {
    (d - Duration::hours(3))
        .format("%Y-%m-%dT%H:%M:%S-03:00")
        .to_string()
}

/// The `<DPS>` document, ready to be signed.
pub fn build_dps(cfg: &DpsConfig, req: &NfseRequest) -> Result<El> {
    let c = &req.customer;
    let now = cfg.now.unwrap_or_else(Utc::now);
    let Some(toma_mun) = filled(&c.city_ibge_code) else {
        return Err(AppError::internal(format!(
            "Cliente {}: código IBGE do município é obrigatório para emitir NFS-e",
            c.name
        )));
    };
    let simples = cfg.op_simp_nac != "1";
    let series = cfg.series.as_deref().unwrap_or("1");

    let mut reg_trib = El::new("regTrib").child(El::leaf("opSimpNac", &cfg.op_simp_nac));
    if cfg.op_simp_nac == "3" {
        reg_trib = reg_trib.maybe(filled(&cfg.reg_ap_trib_sn).map(|v| El::leaf("regApTribSN", v)));
    }
    let prest = El::new("prest")
        .child(El::leaf("CNPJ", &cfg.provider_cnpj))
        .maybe(filled(&cfg.provider_municipal_registration).map(|v| El::leaf("IM", v)))
        .maybe(filled(&cfg.provider_phone).map(|v| El::leaf("fone", v)))
        .maybe(filled(&cfg.provider_email).map(|v| El::leaf("email", v)))
        .child(reg_trib.child(El::leaf("regEspTrib", "0")));

    let toma = El::new("toma")
        .child(El::leaf("CNPJ", &c.cnpj))
        .child(El::leaf("xNome", &c.name))
        .child(
            El::new("end")
                .child(
                    El::new("endNac")
                        .child(El::leaf("cMun", toma_mun))
                        .child(El::leaf("CEP", &c.zipcode)),
                )
                .child(El::leaf("xLgr", &c.address))
                .child(El::leaf("nro", &c.address_number))
                .maybe(filled(&c.address_complement).map(|v| El::leaf("xCpl", v)))
                .child(El::leaf("xBairro", &c.neighborhood)),
        );

    // Rendered at the provider, as the Emissor Web does (ISS due there).
    let c_nbs = || filled(&cfg.nbs_code).map(|v| El::leaf("cNBS", v));
    let mut c_serv = El::new("cServ")
        .child(El::leaf("cTribNac", &cfg.service_code))
        .maybe(filled(&cfg.municipal_service_code).map(|v| El::leaf("cTribMun", v)));
    if cfg.layout_nt009 {
        c_serv = c_serv.maybe(c_nbs());
    }
    c_serv = c_serv.child(El::leaf(
        "xDescServ",
        service_description(&cfg.service_description, &req.competence),
    ));
    if !cfg.layout_nt009 {
        c_serv = c_serv.maybe(c_nbs());
    }
    let serv = El::new("serv")
        .child(El::new("locPrest").child(El::leaf("cLocPrestacao", &cfg.provider_city_ibge)))
        .child(c_serv);

    let mut trib_mun = El::new("tribMun").child(El::leaf("tribISSQN", "1"));
    if !simples {
        trib_mun = trib_mun.child(El::leaf("pAliq", format!("{:.2}", cfg.iss_rate_percent)));
    }
    let mut trib = El::new("trib").child(trib_mun.child(El::leaf("tpRetISSQN", "1")));
    if simples {
        trib = trib.child(
            El::new("tribFed").child(
                El::new("piscofins")
                    .child(El::leaf("CST", "00"))
                    .child(El::leaf("tpRetPisCofins", "0")),
            ),
        );
    }
    trib = trib.child(El::new("totTrib").child(match cfg.tot_trib_sn_percent {
        Some(p) if simples => El::leaf("pTotTribSN", format!("{p:.2}")),
        _ => El::leaf("indTotTrib", "0"),
    }));

    // The billing competence is the month of the service; the note's own
    // competence is the month it is issued (never after dhEmi).
    let issued = brt_iso(now);
    let inf = El::new("infDPS")
        .attr("Id", dps_id(cfg, req.dps_number))
        .child(El::leaf("tpAmb", cfg.tp_amb.to_string()))
        .child(El::leaf("dhEmi", &issued))
        .child(El::leaf("verAplic", "nf-fatura-1.0"))
        .child(El::leaf(
            "serie",
            series.parse::<u64>().unwrap_or(1).to_string(),
        ))
        .child(El::leaf("nDPS", req.dps_number.to_string()))
        .child(El::leaf("dCompet", format!("{}-01", &issued[..7])))
        .child(El::leaf("tpEmit", "1"))
        .maybe(cfg.layout_nt009.then(|| El::leaf("finNFSe", "0")))
        .child(El::leaf("cLocEmi", &cfg.provider_city_ibge))
        .child(prest)
        .child(toma)
        .child(serv)
        .child(
            El::new("valores")
                .child(El::new("vServPrest").child(El::leaf("vServ", money(req.amount_cents))))
                .child(trib),
        );

    Ok(El::new("DPS").ns(NFSE_NS).attr("versao", "1.01").child(inf))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::integrations::types::BillingParty;
    use chrono::TimeZone;

    pub fn cfg() -> DpsConfig {
        DpsConfig {
            tp_amb: 2,
            provider_cnpj: "11222333000181".into(),
            provider_city_ibge: "3550308".into(),
            service_code: "010101".into(),
            service_description: "Serviços & suporte".into(),
            op_simp_nac: "1".into(),
            iss_rate_percent: 2.0,
            now: Some(Utc.with_ymd_and_hms(2026, 9, 28, 13, 0, 0).unwrap()),
            ..DpsConfig::default()
        }
    }

    pub fn req() -> NfseRequest {
        NfseRequest {
            billing_id: "b1".into(),
            dps_number: 7,
            amount_cents: 123456,
            competence: "2026-09".into(),
            customer: BillingParty {
                name: "ACME <Ltda>".into(),
                cnpj: "22333444000181".into(),
                address: "Rua A".into(),
                address_number: "10".into(),
                address_complement: None,
                neighborhood: "Centro".into(),
                city: "SP".into(),
                city_ibge_code: Some("3550308".into()),
                state: "SP".into(),
                zipcode: "01001000".into(),
                station: None,
            },
        }
    }

    #[test]
    fn builds_a_45_char_id() {
        let id = dps_id(&cfg(), 7);
        assert_eq!(id.len(), 45);
        assert_eq!(id, "DPS355030821122233300018100001000000000000007");
    }

    #[test]
    fn escapes_text_formats_money_and_time() {
        let xml = build_dps(&cfg(), &req()).unwrap().to_xml();
        assert!(xml.contains("<xNome>ACME &lt;Ltda&gt;</xNome>"));
        assert!(xml.contains("<vServ>1234.56</vServ>"));
        assert!(xml.contains("<dhEmi>2026-09-28T10:00:00-03:00</dhEmi>"));
        assert!(!xml.contains("<xCpl>"));
    }

    #[test]
    fn matches_the_emissor_web_layout_for_a_simples_me_epp_provider() {
        let cfg = DpsConfig {
            provider_cnpj: "03894412000100".into(),
            provider_city_ibge: "3303302".into(),
            provider_phone: Some("48991491080".into()),
            provider_email: Some("a@b.com".into()),
            service_code: "010701".into(),
            municipal_service_code: Some("001".into()),
            nbs_code: Some("115013000".into()),
            service_description:
                "Serviços de informática prestados Valor aproximado dos tributos: 6%".into(),
            op_simp_nac: "3".into(),
            reg_ap_trib_sn: Some("1".into()),
            tot_trib_sn_percent: Some(6.0),
            series: Some("70000".into()),
            ..cfg()
        };
        let mut req = req();
        req.customer.city_ibge_code = Some("3300803".into());
        let xml = build_dps(&cfg, &req).unwrap().to_xml();
        assert!(xml.contains("<serie>70000</serie>"));
        assert!(xml.contains("<fone>48991491080</fone><email>a@b.com</email>"));
        assert!(xml.contains(
            "<opSimpNac>3</opSimpNac><regApTribSN>1</regApTribSN><regEspTrib>0</regEspTrib>"
        ));
        assert!(xml.contains("<cLocPrestacao>3303302</cLocPrestacao>"));
        assert!(xml.contains("<cTribNac>010701</cTribNac><cTribMun>001</cTribMun>"));
        assert!(xml.contains("</xDescServ><cNBS>115013000</cNBS>"));
        assert!(xml.contains(
            "<tribFed><piscofins><CST>00</CST><tpRetPisCofins>0</tpRetPisCofins></piscofins></tribFed>"
        ));
        assert!(xml.contains("<totTrib><pTotTribSN>6.00</pTotTribSN></totTrib>"));
        assert!(!xml.contains("<pAliq>"));
    }

    #[test]
    fn follows_the_nt009_layout_when_asked() {
        let nt009 = DpsConfig {
            nbs_code: Some("115013000".into()),
            layout_nt009: true,
            ..cfg()
        };
        let xml = build_dps(&nt009, &req()).unwrap().to_xml();
        assert!(xml.contains("<tpEmit>1</tpEmit><finNFSe>0</finNFSe><cLocEmi>"));
        assert!(xml.contains("<cNBS>115013000</cNBS><xDescServ>"));
        let old = DpsConfig {
            layout_nt009: false,
            ..nt009
        };
        assert!(!build_dps(&old, &req()).unwrap().to_xml().contains("<finNFSe>"));
    }

    #[test]
    fn competes_in_the_month_it_is_issued() {
        // Service of October, issued on 2026-11-01 00:30 in Brasília.
        let cfg = DpsConfig {
            now: Some(Utc.with_ymd_and_hms(2026, 11, 1, 3, 30, 0).unwrap()),
            ..cfg()
        };
        let mut req = req();
        req.competence = "2026-10".into();
        let xml = build_dps(&cfg, &req).unwrap().to_xml();
        assert!(xml.contains("<dCompet>2026-11-01</dCompet>"));
    }

    #[test]
    fn fills_the_month_placeholders_of_the_service_description() {
        let cfg = DpsConfig {
            service_description: "Serviços de {mes_anterior} (competência {mes})".into(),
            ..cfg()
        };
        let mut req = req();
        req.competence = "2026-01".into();
        let xml = build_dps(&cfg, &req).unwrap().to_xml();
        assert!(xml.contains(
            "<xDescServ>Serviços de Dezembro de 2025 (competência Janeiro de 2026)</xDescServ>"
        ));
    }

    #[test]
    fn requires_the_customer_ibge_code() {
        let mut req = req();
        req.customer.city_ibge_code = None;
        assert!(build_dps(&cfg(), &req)
            .unwrap_err()
            .message
            .contains("IBGE"));
    }
}
