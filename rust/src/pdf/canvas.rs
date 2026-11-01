//! The small subset of pdfkit the documents need: Helvetica text (measured,
//! aligned, wrapped, clamped with an ellipsis), rectangles and lines, on a
//! top-left origin like pdfkit's.

use super::helvetica::*;
use flate2::{write::ZlibEncoder, Compression};
use std::io::Write;

const PAGE_W: f64 = 595.28;
const PAGE_H: f64 = 841.89;

#[derive(Clone, Copy, PartialEq)]
pub enum Font {
    Regular,
    Bold,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Align {
    #[default]
    Left,
    Right,
    Center,
}

/// Without a width the text is a single line; with one it wraps, like pdfkit.
#[derive(Clone, Copy, Default)]
pub struct TextOpts {
    pub width: Option<f64>,
    pub align: Align,
    /// Lines past this height are dropped.
    pub height: Option<f64>,
    /// Ends the last line that fits with "…" when text was dropped.
    pub ellipsis: bool,
}

impl TextOpts {
    pub fn width(width: f64) -> Self {
        Self {
            width: Some(width),
            ..Self::default()
        }
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn clamp(mut self, height: f64) -> Self {
        self.height = Some(height);
        self.ellipsis = true;
        self
    }
}

/// AFM metrics of a font, per 1000 units of font size.
struct Metrics {
    widths: &'static [u16; 256],
    kerns: &'static [(u8, u8, i16)],
    ascender: f64,
    line_height: f64,
}

pub struct Pdf {
    title: Option<String>,
    pages: Vec<Vec<u8>>,
    cur: Vec<u8>,
    scale: f64,
    font: Font,
    size: f64,
    fill: [f64; 3],
    stroke: [f64; 3],
    line_width: f64,
}

fn rgb(hex: &str) -> [f64; 3] {
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    [
        ((v >> 16) & 0xff) as f64 / 255.0,
        ((v >> 8) & 0xff) as f64 / 255.0,
        (v & 0xff) as f64 / 255.0,
    ]
}

fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// WinAnsi bytes of the text; characters outside the encoding become "?".
fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| {
            let u = c as u32;
            match u {
                0x20..=0x7e | 0xa1..=0xff => u as u8,
                0xa0 => b' ',
                _ => WIN_ANSI_HIGH
                    .iter()
                    .position(|h| *h == u)
                    .map_or(b'?', |i| 0x80 + i as u8),
            }
        })
        .collect()
}

fn kern(table: &[(u8, u8, i16)], l: u8, r: u8) -> i16 {
    table
        .binary_search_by(|(a, b, _)| (*a, *b).cmp(&(l, r)))
        .map_or(0, |i| table[i].2)
}

impl Pdf {
    /// A4 document with one blank page.
    pub fn new(title: Option<&str>) -> Self {
        Self {
            title: title.map(str::to_string),
            pages: Vec::new(),
            cur: Vec::new(),
            scale: 1.0,
            font: Font::Regular,
            size: 12.0,
            fill: [0.0; 3],
            stroke: [0.0; 3],
            line_width: 1.0,
        }
    }

    /// Layout units per point: coordinates and sizes are multiplied by it.
    pub fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Width of the page in layout units.
    pub fn page_width(&self) -> f64 {
        PAGE_W / self.scale
    }

    pub fn add_page(&mut self) {
        self.pages.push(std::mem::take(&mut self.cur));
    }

    pub fn font(&mut self, font: Font, size: f64) -> &mut Self {
        self.font = font;
        self.size = size;
        self
    }

    pub fn fill_color(&mut self, hex: &str) -> &mut Self {
        self.fill = rgb(hex);
        self
    }

    pub fn stroke_color(&mut self, hex: &str) -> &mut Self {
        self.stroke = rgb(hex);
        self
    }

    pub fn line_width(&mut self, width: f64) -> &mut Self {
        self.line_width = width;
        self
    }

    fn metrics(&self) -> Metrics {
        match self.font {
            Font::Regular => Metrics {
                widths: &REGULAR_WIDTHS,
                kerns: &REGULAR_KERN,
                ascender: REGULAR_ASCENDER,
                line_height: REGULAR_LINE_HEIGHT,
            },
            Font::Bold => Metrics {
                widths: &BOLD_WIDTHS,
                kerns: &BOLD_KERN,
                ascender: BOLD_ASCENDER,
                line_height: BOLD_LINE_HEIGHT,
            },
        }
    }

    fn width_of_bytes(&self, bytes: &[u8]) -> f64 {
        let Metrics { widths, kerns, .. } = self.metrics();
        let mut total = 0i64;
        for (i, b) in bytes.iter().enumerate() {
            total += widths[*b as usize] as i64;
            if let Some(next) = bytes.get(i + 1) {
                total += kern(kerns, *b, *next) as i64;
            }
        }
        total as f64 * self.size / 1000.0
    }

    pub fn width_of(&self, text: &str) -> f64 {
        self.width_of_bytes(&encode(text))
    }

    pub fn line_height(&self) -> f64 {
        self.metrics().line_height * self.size / 1000.0
    }

    fn op(&mut self, s: String) {
        self.cur.extend_from_slice(s.as_bytes());
        self.cur.push(b'\n');
    }

    fn x(&self, x: f64) -> f64 {
        x * self.scale
    }

    fn y(&self, y: f64) -> f64 {
        PAGE_H - y * self.scale
    }

    pub fn fill_rect(&mut self, x: f64, y: f64, w: f64, h: f64, hex: &str) {
        let [r, g, b] = rgb(hex);
        let s = self.scale;
        self.op(format!(
            "{} {} {} rg {} {} {} {} re f",
            num(r),
            num(g),
            num(b),
            num(self.x(x)),
            num(self.y(y + h)),
            num(w * s),
            num(h * s)
        ));
    }

    fn stroke_setup(&self) -> String {
        let [r, g, b] = self.stroke;
        format!(
            "{} {} {} RG {} w",
            num(r),
            num(g),
            num(b),
            num(self.line_width * self.scale)
        )
    }

    pub fn stroke_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let s = self.scale;
        self.op(format!(
            "{} {} {} {} {} re S",
            self.stroke_setup(),
            num(self.x(x)),
            num(self.y(y + h)),
            num(w * s),
            num(h * s)
        ));
    }

    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.op(format!(
            "{} {} {} m {} {} l S",
            self.stroke_setup(),
            num(self.x(x1)),
            num(self.y(y1)),
            num(self.x(x2)),
            num(self.y(y2))
        ));
    }

    /// One line with its top at `y`; kerning goes into the TJ array.
    fn show(&mut self, bytes: &[u8], x: f64, y: f64) {
        if bytes.is_empty() {
            return;
        }
        let Metrics {
            kerns, ascender, ..
        } = self.metrics();
        let mut tj = Vec::from(&b"[("[..]);
        for (i, b) in bytes.iter().enumerate() {
            if matches!(b, b'(' | b')' | b'\\') {
                tj.push(b'\\');
            }
            tj.push(*b);
            let k = bytes.get(i + 1).map_or(0, |next| kern(kerns, *b, *next));
            if k != 0 {
                tj.extend_from_slice(format!(") {} (", -k).as_bytes());
            }
        }
        tj.extend_from_slice(b")] TJ");
        let [r, g, b] = self.fill;
        let font = if self.font == Font::Bold { "F2" } else { "F1" };
        let baseline = y + ascender * self.size / 1000.0;
        let head = format!(
            "BT /{font} {} Tf {} {} {} rg 1 0 0 1 {} {} Tm ",
            num(self.size * self.scale),
            num(r),
            num(g),
            num(b),
            num(self.x(x)),
            num(self.y(baseline))
        );
        self.cur.extend_from_slice(head.as_bytes());
        self.cur.extend_from_slice(&tj);
        self.cur.extend_from_slice(b" ET\n");
    }

    /// Greedy wrap at spaces and hyphens; a word wider than the line is cut.
    fn wrap(&self, bytes: &[u8], width: f64) -> Vec<Vec<u8>> {
        let fits = |line: &[u8]| {
            let end = line.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
            self.width_of_bytes(&line[..end]) <= width
        };
        let mut words: Vec<&[u8]> = Vec::new();
        let mut start = 0;
        for i in 0..bytes.len() {
            let last = i + 1 == bytes.len();
            let boundary = match bytes[i] {
                b' ' => bytes.get(i + 1) != Some(&b' '),
                b'-' => true,
                _ => false,
            };
            if boundary || last {
                words.push(&bytes[start..=i]);
                start = i + 1;
            }
        }
        let mut lines: Vec<Vec<u8>> = Vec::new();
        let mut cur: Vec<u8> = Vec::new();
        for word in words {
            let mut word = word;
            loop {
                let candidate = [&cur[..], word].concat();
                if fits(&candidate) {
                    cur = candidate;
                    break;
                }
                if !cur.is_empty() {
                    lines.push(std::mem::take(&mut cur));
                    continue;
                }
                // The word alone is wider than the line.
                let mut n = word.len();
                while n > 1 && !fits(&word[..n]) {
                    n -= 1;
                }
                lines.push(word[..n].to_vec());
                word = &word[n..];
                if word.is_empty() {
                    break;
                }
            }
        }
        if !cur.is_empty() || lines.is_empty() {
            lines.push(cur);
        }
        lines
    }

    /// Draws the text with its top-left corner at (x, y). Returns where a
    /// following single-line text would continue (x right after this one).
    pub fn text(&mut self, text: &str, x: f64, y: f64, opts: TextOpts) -> f64 {
        let bytes = encode(text);
        let Some(width) = opts.width else {
            self.show(&bytes, x, y);
            return x + self.width_of_bytes(&bytes);
        };
        let line_height = self.line_height();
        let mut lines = self.wrap(&bytes, width);
        if let Some(height) = opts.height {
            let max = ((height / line_height).floor() as usize).max(1);
            if lines.len() > max {
                lines.truncate(max);
                if opts.ellipsis {
                    let last = lines.last_mut().unwrap();
                    let trim = |l: &mut Vec<u8>| {
                        while l.last() == Some(&b' ') {
                            l.pop();
                        }
                    };
                    trim(last);
                    const ELLIPSIS: u8 = 0x85;
                    while !last.is_empty()
                        && self.width_of_bytes(&[&last[..], &[ELLIPSIS]].concat()) > width
                    {
                        last.pop();
                        trim(last);
                    }
                    last.push(ELLIPSIS);
                }
            }
        }
        for (i, line) in lines.iter().enumerate() {
            let end = line.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
            let w = self.width_of_bytes(&line[..end]);
            let dx = match opts.align {
                Align::Left => 0.0,
                Align::Right => width - w,
                Align::Center => (width - w) / 2.0,
            };
            self.show(&line[..end], x + dx, y + i as f64 * line_height);
        }
        x
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.add_page();
        let mut out: Vec<u8> = Vec::from(&b"%PDF-1.4\n%\xff\xff\xff\xff\n"[..]);
        let mut offsets: Vec<usize> = Vec::new();
        let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
            out.extend_from_slice(body);
            out.extend_from_slice(b"\nendobj\n");
        };
        // 1 catalog, 2 pages, 3-4 fonts, 5 info, then page + content pairs.
        let kids: Vec<String> = (0..self.pages.len())
            .map(|i| format!("{} 0 R", 6 + i * 2))
            .collect();
        obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
        obj(
            &mut out,
            format!(
                "<< /Type /Pages /Kids [{}] /Count {} >>",
                kids.join(" "),
                self.pages.len()
            )
            .as_bytes(),
        );
        for name in ["Helvetica", "Helvetica-Bold"] {
            obj(
                &mut out,
                format!(
                    "<< /Type /Font /Subtype /Type1 /BaseFont /{name} /Encoding /WinAnsiEncoding >>"
                )
                .as_bytes(),
            );
        }
        let mut info = Vec::from(&b"<< /Producer (NF Fatura)"[..]);
        if let Some(title) = &self.title {
            info.extend_from_slice(b" /Title (");
            for b in encode(title) {
                if matches!(b, b'(' | b')' | b'\\') {
                    info.push(b'\\');
                }
                info.push(b);
            }
            info.push(b')');
        }
        info.extend_from_slice(b" >>");
        obj(&mut out, &info);
        for (i, content) in self.pages.iter().enumerate() {
            obj(
                &mut out,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                     /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                    7 + i * 2
                )
                .as_bytes(),
            );
            let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
            z.write_all(content).expect("in-memory write");
            let data = z.finish().expect("in-memory write");
            let mut body = format!(
                "<< /Length {} /Filter /FlateDecode >>\nstream\n",
                data.len()
            )
            .into_bytes();
            body.extend_from_slice(&data);
            body.extend_from_slice(b"\nendstream");
            obj(&mut out, &body);
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n", offsets.len() + 1).as_bytes());
        out.extend_from_slice(b"0000000000 65535 f \n");
        for o in &offsets {
            out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                offsets.len() + 1
            )
            .as_bytes(),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_with_the_afm_widths_and_kerning() {
        let mut pdf = Pdf::new(None);
        pdf.font(Font::Regular, 10.0);
        // 556 * 3 per 1000 at size 10
        assert!((pdf.width_of("000") - 16.68).abs() < 1e-9);
        // "AV" kerns by -70
        assert!((pdf.width_of("AV") - (667.0 + 667.0 - 70.0) / 100.0).abs() < 1e-9);
    }

    #[test]
    fn wraps_and_clamps_with_an_ellipsis() {
        let mut pdf = Pdf::new(None);
        pdf.font(Font::Regular, 10.0);
        let bytes = encode("aaa bbb ccc ddd");
        let width = pdf.width_of("aaa bbb");
        let lines = pdf.wrap(&bytes, width);
        assert_eq!(lines, vec![b"aaa bbb ".to_vec(), b"ccc ddd".to_vec()]);
        pdf.text(
            "aaa bbb ccc ddd",
            0.0,
            0.0,
            TextOpts::width(width).clamp(12.0),
        );
        let content = String::from_utf8_lossy(&pdf.cur).to_string();
        assert!(content.contains("\u{fffd}") || pdf.cur.contains(&0x85));
        assert_eq!(content.matches("TJ").count(), 1);
    }
}
