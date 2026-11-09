use chrono::{Datelike, NaiveDate};

pub struct Range {
    pub from: String,
    pub to: String,
}

fn iso_day(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// First and last day (YYYY-MM-DD) of a YYYY-MM competence.
pub fn month_range(competence: &str) -> Range {
    let mut parts = competence.split('-').map(|p| p.parse::<u32>().unwrap_or(0));
    let y = parts.next().unwrap_or(0) as i32;
    let m = parts.next().unwrap_or(1).clamp(1, 12);
    let next = if m == 12 {
        NaiveDate::from_ymd_opt(y + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(y, m + 1, 1)
    };
    let last = next.and_then(|d| d.pred_opt()).map_or(28, |d| d.day());
    Range {
        from: format!("{y}-{m:02}-01"),
        to: format!("{y}-{m:02}-{last:02}"),
    }
}

/// Same as month_range, but the end never goes past today (the current month
/// is still open).
pub fn month_range_until_today(competence: &str, today: NaiveDate) -> Range {
    let Range { from, to } = month_range(competence);
    let t = iso_day(today);
    Range {
        from,
        to: if to > t { t } else { to },
    }
}

const MONTHS: [&str; 12] = [
    "Janeiro",
    "Fevereiro",
    "Março",
    "Abril",
    "Maio",
    "Junho",
    "Julho",
    "Agosto",
    "Setembro",
    "Outubro",
    "Novembro",
    "Dezembro",
];

fn month_index(competence: &str, delta: i32) -> i32 {
    let mut parts = competence.split('-').map(|p| p.parse::<i32>().unwrap_or(0));
    let y = parts.next().unwrap_or(0);
    let m = parts.next().unwrap_or(1).clamp(1, 12);
    y * 12 + (m - 1) + delta
}

/// The YYYY-MM competence `delta` months away.
pub fn shift_month(competence: &str, delta: i32) -> String {
    let i = month_index(competence, delta);
    format!("{}-{:02}", i.div_euclid(12), i.rem_euclid(12) + 1)
}

/// "Outubro de 2026" for 2026-10, shifted by `delta` months.
pub fn month_label(competence: &str, delta: i32) -> String {
    let i = month_index(competence, delta);
    format!("{} de {}", MONTHS[i.rem_euclid(12) as usize], i.div_euclid(12))
}

/// True when the month has not started yet.
pub fn is_future_month(competence: &str, today: NaiveDate) -> bool {
    month_range(competence).from > iso_day(today)
}

/// YYYY-MM with a valid month.
pub fn is_competence(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 7
        && b[4] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..].iter().all(u8::is_ascii_digit)
        && (1..=12).contains(&v[5..].parse::<u32>().unwrap_or(0))
}

/// YYYY-MM-DD (shape only, like the API).
pub fn is_date(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn shifts_months_across_years() {
        assert_eq!(shift_month("2026-01", -1), "2025-12");
        assert_eq!(shift_month("2026-12", 1), "2027-01");
        assert_eq!(month_label("2026-03", -1), "Fevereiro de 2026");
    }

    #[test]
    fn computes_month_ranges_including_leap_years() {
        let r = month_range("2026-09");
        assert_eq!(
            (r.from.as_str(), r.to.as_str()),
            ("2026-09-01", "2026-09-30")
        );
        assert_eq!(month_range("2028-02").to, "2028-02-29");
        assert_eq!(month_range("2026-12").to, "2026-12-31");
    }

    #[test]
    fn caps_the_current_month_at_today_and_leaves_past_months_whole() {
        let r = month_range_until_today("2026-10", day(2026, 10, 15));
        assert_eq!(
            (r.from.as_str(), r.to.as_str()),
            ("2026-10-01", "2026-10-15")
        );
        assert_eq!(
            month_range_until_today("2026-09", day(2026, 10, 15)).to,
            "2026-09-30"
        );
    }

    #[test]
    fn only_rejects_months_that_have_not_started() {
        assert!(!is_future_month("2026-10", day(2026, 10, 1)));
        assert!(is_future_month("2026-11", day(2026, 10, 31)));
    }

    #[test]
    fn validates_competence_and_date_shapes() {
        assert!(is_competence("2026-09"));
        assert!(!is_competence("2026-13"));
        assert!(!is_competence("2026-9"));
        assert!(is_date("2026-09-05"));
        assert!(!is_date("05/09/2026"));
    }
}
