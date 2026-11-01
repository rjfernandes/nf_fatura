//! Renders the DANFSe (auxiliary document of the NFS-e) from the authorized
//! NFS-e XML, following the layout of the national DANFSe v2.0.
//!
//! Coordinates below are in the units of the reference layout (1413 px wide);
//! the page is scaled down to A4.

use super::parse::{invalid_nfse, sections, text};
use crate::error::Result;
use crate::pdf::canvas::{Align, Font, Pdf, TextOpts};
use crate::util::brl_number;
use roxmltree::{Document, Node};

#[derive(Default)]
pub struct DanfseExtras {
    /// The XML has no city name for the customer; the DB does.
    pub customer_city: Option<String>,
    pub customer_state: Option<String>,
}

const W: f64 = 1413.0;
const GRAY: &str = "#f2f2f2";
const COL: [f64; 4] = [28.0, 372.0, 715.0, 1058.0];
const LEFT: f64 = 20.0;
const RIGHT: f64 = 1394.0;
const BOX_W: f64 = 344.0; // gray title box (LEFT..364)

fn digits(v: &str) -> String {
    v.chars().filter(char::is_ascii_digit).collect()
}

fn dash(v: Option<String>) -> String {
    match v {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => "-".into(),
    }
}

/// Groups the digits by the given sizes, joined by the separators.
fn mask(d: &str, sizes: &[usize], seps: &[&str]) -> String {
    let mut out = String::new();
    let mut at = 0;
    for (i, size) in sizes.iter().enumerate() {
        if i > 0 {
            out.push_str(seps[i - 1]);
        }
        out.push_str(&d[at..at + size]);
        at += size;
    }
    out
}

fn cnpj_cpf(v: Option<String>) -> String {
    let d = digits(v.as_deref().unwrap_or(""));
    match d.len() {
        14 => mask(&d, &[2, 3, 3, 4, 2], &[".", ".", "/", "-"]),
        11 => mask(&d, &[3, 3, 3, 2], &[".", ".", "-"]),
        _ => dash(v),
    }
}

fn phone(v: Option<String>) -> String {
    let d = digits(v.as_deref().unwrap_or(""));
    match d.len() {
        11 => format!("({}) {}-{}", &d[..2], &d[2..7], &d[7..]),
        10 => format!("({}) {}-{}", &d[..2], &d[2..6], &d[6..]),
        _ => dash(v),
    }
}

fn cep(v: Option<String>) -> String {
    let d = digits(v.as_deref().unwrap_or(""));
    if d.len() == 8 {
        mask(&d, &[2, 3, 3], &[".", "-"])
    } else {
        dash(v)
    }
}

fn ibge(v: Option<String>) -> String {
    let d = digits(v.as_deref().unwrap_or(""));
    if d.len() == 7 {
        mask(&d, &[2, 5], &["."])
    } else {
        dash(v)
    }
}

fn brl(v: Option<String>) -> String {
    match v.and_then(|v| v.parse::<f64>().ok()) {
        Some(n) => {
            let cents = (n * 100.0).round() as i64;
            format!(
                "{}R$ {}",
                if cents < 0 { "-" } else { "" },
                brl_number(cents)
            )
        }
        None => "-".into(),
    }
}

fn date(iso: Option<&str>) -> String {
    match iso {
        Some(iso) if iso.len() >= 10 => iso[..10].split('-').rev().collect::<Vec<_>>().join("/"),
        _ => "-".into(),
    }
}

fn date_time(iso: Option<&str>) -> String {
    match iso {
        Some(iso) if iso.len() >= 19 => format!("{} {}", date(Some(iso)), &iso[11..19]),
        Some(iso) => date(Some(iso)),
        None => "-".into(),
    }
}

fn address(node: Option<Node>) -> String {
    let Some(node) = node else {
        return "-".into();
    };
    ["xLgr", "nro", "xCpl", "xBairro"]
        .iter()
        .filter_map(|k| text(node, &[k]))
        .collect::<Vec<_>>()
        .join(", ")
}

fn simples(v: &str) -> Option<&'static str> {
    Some(match v {
        "1" => "Não Optante",
        "2" => "Optante - Microempreendedor Individual (MEI)",
        "3" => "Optante - Microempresa ou Empresa de Pequeno Porte (ME/EPP)",
        _ => return None,
    })
}

fn reg_ap(v: &str) -> Option<&'static str> {
    Some(match v {
        "1" => "Regime de apuração dos tributos federais e municipal pelo Simples Nacional",
        "2" => "Regime de apuração dos tributos federais pelo Simples Nacional e ISSQN pela NFS-e conforme respectivas legislações municipais do tributo",
        "3" => "Regime de apuração dos tributos federais e municipal pela NFS-e conforme respectivas legislações federal e municipal de cada tributo",
        _ => return None,
    })
}

fn ret_iss(v: &str) -> Option<&'static str> {
    Some(match v {
        "1" => "Não Retido",
        "2" => "Retido pelo Tomador",
        "3" => "Retido pelo Intermediário",
        _ => return None,
    })
}

fn trib_iss(v: &str) -> Option<&'static str> {
    Some(match v {
        "1" => "Operação Tributável",
        "2" => "Imunidade",
        "3" => "Exportação de Serviço",
        "4" => "Não Incidência",
        _ => return None,
    })
}

fn pis_cofins_ret(v: &str) -> Option<&'static str> {
    Some(match v {
        "0" => "0 - PIS/COFINS/CSLL Não Retidos",
        "3" => "3 - PIS/COFINS Retidos",
        "4" => "4 - PIS/COFINS/CSLL Retidos",
        _ => return None,
    })
}

fn lookup(v: Option<String>, table: fn(&str) -> Option<&'static str>) -> String {
    v.and_then(|v| table(&v)).unwrap_or("-").to_string()
}

struct Sheet {
    doc: Pdf,
}

impl Sheet {
    fn bold(&mut self, size: f64) -> &mut Pdf {
        self.doc.font(Font::Bold, size)
    }

    fn reg(&mut self, size: f64) -> &mut Pdf {
        self.doc.font(Font::Regular, size)
    }

    fn hline(&mut self, y: f64) {
        self.doc.line_width(1.0).line(LEFT, y, RIGHT, y);
    }

    fn gray(&mut self, x: f64, y: f64, w: f64, h: f64) {
        self.doc.fill_rect(x, y, w, h, GRAY);
    }

    /// Small bold label + value below it.
    fn field_w(&mut self, x: f64, y: f64, label: &str, value: &str, w: f64, big: bool) {
        self.bold(if big { 16.0 } else { 13.0 })
            .text(label, x, y, TextOpts::width(w));
        self.reg(16.0).text(
            value,
            x,
            y + if big { 19.0 } else { 17.0 },
            TextOpts::width(w).clamp(20.0),
        );
    }

    fn field(&mut self, x: f64, y: f64, label: &str, value: &str) {
        self.field_w(x, y, label, value, 330.0, false);
    }

    fn big(&mut self, x: f64, y: f64, label: &str, value: &str) {
        self.field_w(x, y, label, value, 330.0, true);
    }

    fn section_title(&mut self, y: f64, title: &str) {
        self.gray(LEFT, y, BOX_W, 46.0);
        self.bold(17.0)
            .text(title, COL[0], y + 2.0, TextOpts::default());
    }

    /// QR code in a square box, with a one-module quiet zone.
    fn qr(&mut self, data: &str, x: f64, y: f64, size: f64) -> Result<()> {
        let code = qrcode::QrCode::new(data.as_bytes()).map_err(invalid_nfse)?;
        let n = code.width();
        let module = size / (n as f64 + 2.0);
        for (i, color) in code.to_colors().iter().enumerate() {
            if *color == qrcode::Color::Dark {
                self.doc.fill_rect(
                    x + module * (1.0 + (i % n) as f64),
                    y + module * (1.0 + (i / n) as f64),
                    // A hair wider so neighbours leave no seam.
                    module + 0.05,
                    module + 0.05,
                    "#000000",
                );
            }
        }
        Ok(())
    }
}

pub fn render_danfse(xml: &str, extras: &DanfseExtras) -> Result<Vec<u8>> {
    let parsed = Document::parse(xml).map_err(invalid_nfse)?;
    let (inf, dps) = sections(&parsed)?;
    let t = |node: Node, names: &[&str]| text(node, names);
    let id = inf.attribute("Id").unwrap_or("");
    let chave = id.strip_prefix("NFS").unwrap_or(id).to_string();

    let emit_uf = t(inf, &["emit", "enderNac", "UF"]).unwrap_or_default();
    let loc_emi = t(inf, &["xLocEmi"]).unwrap_or_default();
    let toma_city = extras
        .customer_city
        .clone()
        .or_else(|| t(inf, &["xLocPrestacao"]));
    let toma_uf = extras.customer_state.clone().unwrap_or_default();
    let toma_city_uf = [toma_city.unwrap_or_default(), toma_uf.clone()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" / ");
    let p_aliq = t(dps, &["valores", "trib", "tribMun", "pAliq"]);
    let n_nfse = dash(t(inf, &["nNFSe"]));

    let mut doc = Pdf::new(Some(&format!("NFS-e {n_nfse}")));
    doc.set_scale(595.28 / W);
    let mut s = Sheet { doc };
    let one = TextOpts::default();
    let centered = |w: f64| TextOpts::width(w).align(Align::Center);

    // Page frame and header
    s.doc
        .line_width(2.0)
        .stroke_rect(12.0, 12.0, 1389.0, 1976.0);
    s.gray(LEFT, 14.0, RIGHT - LEFT, 80.0);
    s.bold(54.0)
        .fill_color("#4e9a6a")
        .text("NFS", 26.0, 24.0, one);
    s.bold(54.0)
        .fill_color("#2e3a8c")
        .text("e", 133.0, 38.0, one);
    s.reg(18.0)
        .fill_color("#7c85a6")
        .text("Nota Fiscal de", 170.0, 44.0, one);
    s.doc.text("Serviço eletrônica", 170.0, 62.0, one);
    s.doc.fill_color("#000000");
    s.bold(21.0)
        .text("DANFSe v2.0", 400.0, 32.0, centered(610.0));
    s.bold(21.0)
        .text("Documento Auxiliar da NFS-e", 400.0, 56.0, centered(610.0));
    let municipio = if emit_uf.is_empty() {
        format!("Município: {loc_emi}")
    } else {
        format!("Município: {loc_emi} - {emit_uf}")
    };
    s.reg(19.0).text(&municipio, 1058.0, 29.0, one);
    s.reg(13.0).text(
        &format!("Ambiente Gerador: {}", dash(t(inf, &["ambGer"]))),
        1058.0,
        50.0,
        one,
    );
    s.reg(13.0).text(
        &format!("Tipo de Ambiente: {}", dash(t(dps, &["tpAmb"]))),
        1058.0,
        66.0,
        one,
    );
    s.hline(95.0);

    // Identification
    s.field_w(
        COL[0],
        108.0,
        "CHAVE DE ACESSO DA NFS-e",
        &chave,
        700.0,
        true,
    );
    s.qr(
        &format!("https://www.nfse.gov.br/ConsultaPublica/?tpc=1&chave={chave}"),
        1172.0,
        108.0,
        100.0,
    )?;
    s.reg(13.0).text(
        "A autenticidade desta NFS-e pode ser verificada pela leitura deste código QR ou pela consulta da chave de acesso no portal nacional da NFS-e",
        1058.0,
        220.0,
        TextOpts::width(330.0),
    );
    s.big(COL[0], 156.0, "NÚMERO DA NFS-e", &n_nfse);
    s.big(
        COL[1],
        156.0,
        "COMPETÊNCIA DA NFS-e",
        &date(t(dps, &["dCompet"]).as_deref()),
    );
    s.big(
        COL[2],
        156.0,
        "DATA E HORA DA EMISSÃO DA NFS-e",
        &date_time(t(inf, &["dhProc"]).as_deref()),
    );
    s.big(COL[0], 203.0, "NÚMERO DA DPS", &dash(t(dps, &["nDPS"])));
    s.big(COL[1], 203.0, "SÉRIE DA DPS", &dash(t(dps, &["serie"])));
    s.big(
        COL[2],
        203.0,
        "DATA E HORA DA EMISSÃO DA DPS",
        &date_time(t(dps, &["dhEmi"]).as_deref()),
    );
    s.gray(LEFT, 249.0, BOX_W, 49.0);
    s.big(
        COL[0],
        251.0,
        "EMITENTE DA NFS-e",
        if t(dps, &["tpEmit"]).as_deref() == Some("1") {
            "Prestador"
        } else {
            "-"
        },
    );
    let c_stat = t(inf, &["cStat"]);
    s.big(
        COL[1],
        251.0,
        "SITUAÇÃO DA NFS-e",
        &if c_stat.as_deref() == Some("100") {
            "NFS-e Gerada".to_string()
        } else {
            dash(c_stat)
        },
    );
    s.big(COL[2], 251.0, "FINALIDADE", "-");
    s.hline(298.0);

    // Prestador
    s.section_title(299.0, "PRESTADOR / FORNECEDOR");
    s.field(
        COL[1],
        300.0,
        "CNPJ / CPF / NIF",
        &cnpj_cpf(t(inf, &["emit", "CNPJ"]).or_else(|| t(inf, &["emit", "CPF"]))),
    );
    s.field(
        COL[2],
        300.0,
        "Indicador Municipal (Inscrição)",
        &dash(t(dps, &["prest", "IM"])),
    );
    s.field(COL[3], 300.0, "Telefone", &phone(t(inf, &["emit", "fone"])));
    s.field_w(
        COL[0],
        346.0,
        "Nome / Nome Empresarial",
        &dash(t(inf, &["emit", "xNome"])),
        680.0,
        false,
    );
    s.field(
        COL[2],
        346.0,
        "Município / Sigla UF",
        &format!("{loc_emi} / {emit_uf}"),
    );
    s.field(
        COL[3],
        346.0,
        "Código IBGE / CEP",
        &format!(
            "{} / {}",
            ibge(t(inf, &["emit", "enderNac", "cMun"])),
            cep(t(inf, &["emit", "enderNac", "CEP"]))
        ),
    );
    s.field_w(
        COL[0],
        391.0,
        "Endereço",
        &address(super::parse::path(inf, &["emit", "enderNac"])),
        680.0,
        false,
    );
    s.field(
        COL[2],
        391.0,
        "E-mail",
        &dash(t(inf, &["emit", "email"]).map(|e| e.to_lowercase())),
    );
    let op_simp_nac = t(dps, &["prest", "regTrib", "opSimpNac"]);
    s.field_w(
        COL[0],
        436.0,
        "Simples Nacional na Data de Competência",
        &lookup(op_simp_nac.clone(), simples),
        335.0,
        false,
    );
    s.field_w(
        COL[1],
        436.0,
        "Regime de Apuração Tributária pelo SN",
        &if op_simp_nac.as_deref() == Some("3") {
            lookup(t(dps, &["prest", "regTrib", "regApTribSN"]), reg_ap)
        } else {
            "-".into()
        },
        1000.0,
        false,
    );
    s.hline(481.0);

    // Tomador
    s.section_title(482.0, "TOMADOR / ADQUIRENTE");
    s.field(
        COL[1],
        483.0,
        "CNPJ / CPF / NIF",
        &cnpj_cpf(t(dps, &["toma", "CNPJ"]).or_else(|| t(dps, &["toma", "CPF"]))),
    );
    s.field(COL[2], 483.0, "Indicador Municipal (Inscrição)", "-");
    s.field(COL[3], 483.0, "Telefone", &phone(t(dps, &["toma", "fone"])));
    s.field_w(
        COL[0],
        528.0,
        "Nome / Nome Empresarial",
        &dash(t(dps, &["toma", "xNome"])),
        680.0,
        false,
    );
    s.field(
        COL[2],
        528.0,
        "Município / Sigla UF",
        &dash(Some(toma_city_uf)),
    );
    s.field(
        COL[3],
        528.0,
        "Código IBGE / CEP",
        &format!(
            "{} / {}",
            ibge(t(dps, &["toma", "end", "endNac", "cMun"])),
            cep(t(dps, &["toma", "end", "endNac", "CEP"]))
        ),
    );
    s.field_w(
        COL[0],
        573.0,
        "Endereço",
        &address(super::parse::path(dps, &["toma", "end"])),
        680.0,
        false,
    );
    s.field(
        COL[2],
        573.0,
        "E-mail",
        &dash(t(dps, &["toma", "email"]).map(|e| e.to_lowercase())),
    );
    s.hline(617.0);

    s.reg(16.0).text(
        "DESTINATÁRIO DA OPERAÇÃO NÃO IDENTIFICADO NA NFS-e",
        LEFT,
        620.0,
        centered(RIGHT - LEFT),
    );
    s.hline(637.0);
    s.reg(16.0).text(
        "INTERMEDIÁRIO DA OPERAÇÃO NÃO IDENTIFICADO NA NFS-e",
        LEFT,
        640.0,
        centered(RIGHT - LEFT),
    );
    s.hline(657.0);

    // Serviço
    let c_trib_nac = {
        let d = digits(&t(dps, &["serv", "cServ", "cTribNac"]).unwrap_or_default());
        if d.len() == 6 {
            mask(&d, &[2, 2, 2], &[".", "."])
        } else {
            d
        }
    };
    let nbs = {
        let d = digits(&t(dps, &["serv", "cServ", "cNBS"]).unwrap_or_default());
        if d.len() == 9 {
            mask(&d, &[1, 4, 2, 2], &[".", ".", "."])
        } else {
            d
        }
    };
    s.section_title(658.0, "SERVIÇO PRESTADO");
    s.field(
        COL[1],
        659.0,
        "Código de Tributação Nacional/Municipal",
        &format!(
            "{c_trib_nac} / {}",
            dash(t(dps, &["serv", "cServ", "cTribMun"]))
        ),
    );
    s.field(COL[2], 659.0, "Código da NBS", &dash(Some(nbs)));
    s.field(
        COL[3],
        659.0,
        "Local da Prestação / Sigla UF / País",
        &format!(
            "{}{} / -",
            dash(t(inf, &["xLocPrestacao"])),
            if toma_uf.is_empty() {
                String::new()
            } else {
                format!(" / {toma_uf}")
            }
        ),
    );
    s.reg(16.0).text(
        &dash(t(inf, &["xTribNac"])),
        COL[0],
        704.0,
        TextOpts::width(1340.0).clamp(20.0),
    );
    s.field_w(
        COL[0],
        735.0,
        "Descrição do Serviço",
        &dash(t(dps, &["serv", "cServ", "xDescServ"])),
        1340.0,
        false,
    );
    s.hline(778.0);

    // Tributação municipal
    s.section_title(779.0, "TRIBUTAÇÃO MUNICIPAL (ISSQN)");
    s.field(
        COL[1],
        780.0,
        "Tipo de Tributação do ISSQN",
        &lookup(
            t(dps, &["valores", "trib", "tribMun", "tribISSQN"]),
            trib_iss,
        ),
    );
    s.field_w(
        COL[2],
        780.0,
        "Município / Sigla UF / País de Incidência do ISSQN",
        &format!("{} / {emit_uf} / -", dash(t(inf, &["xLocIncid"]))),
        340.0,
        false,
    );
    s.field(COL[0], 826.0, "BC ISSQN", "-");
    s.field(
        COL[1],
        826.0,
        "Alíquota Aplicada",
        &match p_aliq.and_then(|p| p.parse::<f64>().ok()) {
            Some(p) => format!("{p:.2}%").replace('.', ","),
            None => "-".into(),
        },
    );
    s.field(
        COL[2],
        826.0,
        "Retenção do ISSQN",
        &lookup(
            t(dps, &["valores", "trib", "tribMun", "tpRetISSQN"]),
            ret_iss,
        ),
    );
    s.field(COL[3], 826.0, "ISSQN Apurado", "-");
    s.hline(870.0);

    // Tributação federal
    s.section_title(871.0, "TRIBUTAÇÃO FEDERAL (EXCETO CBS)");
    s.field(COL[1], 872.0, "IRRF", "-");
    s.field(COL[2], 872.0, "Contribuição Previdenciária - Retida", "-");
    s.field(COL[3], 872.0, "Contribuições Sociais - Retidas", "-");
    s.field(COL[0], 917.0, "PIS - Débito Apuração Própria", "-");
    s.field(COL[1], 917.0, "COFINS - Débito Apuração Própria", "-");
    s.field_w(
        COL[2],
        917.0,
        "Descrição Contrib. Sociais - Retidas",
        &lookup(
            t(
                dps,
                &["valores", "trib", "tribFed", "piscofins", "tpRetPisCofins"],
            ),
            pis_cofins_ret,
        ),
        340.0,
        false,
    );
    s.hline(962.0);

    // IBS/CBS (not applicable yet)
    s.section_title(963.0, "TRIBUTAÇÃO IBS/CBS");
    s.field(COL[1], 964.0, "CST / cClassTrib", "- / -");
    s.field_w(
        COL[2],
        964.0,
        "Indicador de Operação / Código IBGE Incidência / Município Incidência / Sigla UF",
        "- / - / - / -",
        680.0,
        false,
    );
    s.field(
        COL[0],
        1009.0,
        "Exclusões e Reduções da Base de Cálculo",
        "R$ 0,00",
    );
    s.field(
        COL[1],
        1009.0,
        "Base de Cálculo Após Exclusões e Reduções",
        "-",
    );
    s.field(
        COL[2],
        1009.0,
        "Red. Alíquota IBS / Red. Alíquota CBS",
        "- / -",
    );
    s.field(COL[3], 1009.0, "Alíquota - IBS UF / IBS Mun", "- / -");
    s.field(COL[0], 1055.0, "Alíq. Efetiva Municipal - IBS", "-");
    s.field(COL[1], 1055.0, "Valor Apurado Municipal - IBS", "-");
    s.field(COL[2], 1055.0, "Alíq. Efetiva Estadual - IBS", "-");
    s.field(COL[3], 1055.0, "Valor Apurado Estadual - IBS", "-");
    s.field(COL[0], 1100.0, "Valor Total Apurado - IBS", "-");
    s.field(COL[1], 1100.0, "Alíquota - CBS", "-");
    s.field(COL[2], 1100.0, "Alíquota Efetiva - CBS", "-");
    s.field(COL[3], 1100.0, "Valor Total Apurado - CBS", "-");
    s.hline(1144.0);

    // Totals
    s.section_title(1145.0, "VALOR TOTAL DA NFS-e");
    s.field(
        COL[1],
        1146.0,
        "VALOR DA OPERAÇÃO / SERVIÇO",
        &brl(t(dps, &["valores", "vServPrest", "vServ"])),
    );
    s.field(COL[2], 1146.0, "Desconto Incondicionado", "-");
    s.field(COL[3], 1146.0, "Desconto Condicionado", "-");
    s.field(
        COL[0],
        1190.0,
        "Total das Retenções (ISSQN / Federais)",
        "-",
    );
    s.field(
        COL[1],
        1190.0,
        "VALOR LÍQUIDO DA NFS-e",
        &brl(t(inf, &["valores", "vLiq"])),
    );
    s.field(COL[2], 1190.0, "Total do IBS/CBS", "R$ 0,00");
    s.gray(1050.0, 1190.0, RIGHT - 1050.0, 46.0);
    s.field(
        COL[3],
        1190.0,
        "VALOR LÍQUIDO DA NFS-e + IBS/CBS",
        "R$ 0,00",
    );
    s.hline(1237.0);

    s.bold(17.0)
        .text("INFORMAÇÕES COMPLEMENTARES", COL[0], 1240.0, one);
    s.reg(16.0).text(
        "Totais aproximados dos Tributos cfe. Lei n° 12.741/2012: Federais: -; Estaduais: -; Municipais: -;",
        COL[0],
        1287.0,
        TextOpts::width(1340.0),
    );

    // Footer box
    s.doc
        .line_width(3.0)
        .stroke_rect(LEFT, 1890.0, RIGHT - LEFT, 48.0);
    s.doc.line_width(2.0).line(365.0, 1890.0, 365.0, 1938.0);
    s.doc.line(708.0, 1890.0, 708.0, 1938.0);
    s.bold(14.0).text("DATA CIENTIFICAÇÃO:", 31.0, 1893.0, one);
    s.bold(14.0)
        .text("IDENTIFICAÇÃO E ASSINATURA", 373.0, 1893.0, one);
    s.bold(14.0)
        .text("N° NFS-e / CHAVE NFS-e", 717.0, 1893.0, one);
    s.reg(16.0)
        .text(&format!("{n_nfse} / {chave}"), 717.0, 1910.0, one);

    Ok(s.doc.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_identifiers_like_the_official_document() {
        assert_eq!(
            cnpj_cpf(Some("03894412000100".into())),
            "03.894.412/0001-00"
        );
        assert_eq!(cnpj_cpf(Some("12345678901".into())), "123.456.789-01");
        assert_eq!(phone(Some("48991491080".into())), "(48) 99149-1080");
        assert_eq!(cep(Some("24020000".into())), "24.020-000");
        assert_eq!(ibge(Some("3303302".into())), "33.03302");
        assert_eq!(brl(Some("1234.5".into())), "R$ 1.234,50");
        assert_eq!(brl(None), "-");
        assert_eq!(
            date_time(Some("2026-09-30T23:50:00-03:00")),
            "30/09/2026 23:50:00"
        );
    }

    #[test]
    fn renders_a_pdf_from_the_nfse_xml() {
        let xml = r#"<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse"><infNFSe Id="NFS33033022203894412000100000000000004226095550113703">
<xLocEmi>Niterói</xLocEmi><xLocPrestacao>Rio de Janeiro</xLocPrestacao><nNFSe>42</nNFSe><cStat>100</cStat>
<dhProc>2026-10-01T00:10:00-03:00</dhProc>
<emit><CNPJ>03894412000100</CNPJ><xNome>MRF INFORMATICA LTDA</xNome><enderNac><xLgr>Rua A</xLgr><nro>1</nro><cMun>3303302</cMun><UF>RJ</UF><CEP>24020000</CEP></enderNac></emit>
<valores><vLiq>1234.50</vLiq></valores>
<DPS><infDPS><tpAmb>2</tpAmb><dhEmi>2026-09-30T23:50:00-03:00</dhEmi><serie>70000</serie><nDPS>15</nDPS><dCompet>2026-09-01</dCompet><tpEmit>1</tpEmit>
<prest><regTrib><opSimpNac>3</opSimpNac><regApTribSN>1</regApTribSN></regTrib></prest>
<toma><CNPJ>99888777000166</CNPJ><xNome>Cliente &amp; Cia</xNome><end><endNac><cMun>3304557</cMun><CEP>20000000</CEP></endNac><xLgr>Rua B</xLgr><nro>2</nro><xBairro>Centro</xBairro></end></toma>
<serv><cServ><cTribNac>010701</cTribNac><cTribMun>001</cTribMun><xDescServ>Serviços de informática</xDescServ><cNBS>115013000</cNBS></cServ></serv>
<valores><vServPrest><vServ>1234.50</vServ></vServPrest><trib><tribMun><tribISSQN>1</tribISSQN><tpRetISSQN>1</tpRetISSQN></tribMun></trib></valores>
</infDPS></DPS></infNFSe></NFSe>"#;
        let pdf = render_danfse(xml, &DanfseExtras::default()).unwrap();
        assert_eq!(&pdf[..5], b"%PDF-");
        assert!(pdf.len() > 2000);
        assert!(render_danfse("<x/>", &DanfseExtras::default()).is_err());
    }
}
