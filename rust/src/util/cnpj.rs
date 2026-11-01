pub fn only_digits(v: &str) -> String {
    v.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Keeps letters and digits, uppercased: the CNPJ can be alphanumeric from 2026.
pub fn normalize_cnpj(v: &str) -> String {
    v.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn check_digit(values: &[u32]) -> u32 {
    // Weights 2..9 from the right, cycling.
    let sum: u32 = values
        .iter()
        .rev()
        .enumerate()
        .map(|(i, v)| v * (2 + (i as u32 % 8)))
        .sum();
    match sum % 11 {
        r if r < 2 => 0,
        r => 11 - r,
    }
}

/// 12 letters/digits (root + order) followed by 2 numeric check digits.
pub fn is_valid_cnpj(v: &str) -> bool {
    let cnpj = normalize_cnpj(v);
    let bytes = cnpj.as_bytes();
    if bytes.len() != 14 || !bytes[12..].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if bytes.iter().all(|b| *b == bytes[0]) {
        return false;
    }
    let mut values: Vec<u32> = bytes[..12].iter().map(|b| (*b - b'0') as u32).collect();
    let dv1 = check_digit(&values);
    values.push(dv1);
    let dv2 = check_digit(&values);
    dv1 == (bytes[12] - b'0') as u32 && dv2 == (bytes[13] - b'0') as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_numeric_cnpjs_formatted_or_not() {
        assert!(is_valid_cnpj("11.222.333/0001-81"));
        assert!(is_valid_cnpj("11222333000181"));
    }

    #[test]
    fn accepts_valid_alphanumeric_cnpjs_in_any_case() {
        assert!(is_valid_cnpj("12.ABC.345/01DE-35"));
        assert!(is_valid_cnpj("12abc34501de35"));
    }

    #[test]
    fn rejects_invalid_ones() {
        assert!(!is_valid_cnpj("11222333000182"));
        assert!(!is_valid_cnpj("12.ABC.345/01DE-36"));
        assert!(!is_valid_cnpj("00000000000000"));
        assert!(!is_valid_cnpj("123"));
    }

    #[test]
    fn normalizes_to_uppercase_alphanumerics() {
        assert_eq!(normalize_cnpj("12.abc.345/01de-35"), "12ABC34501DE35");
    }
}
