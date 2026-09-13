//! Byte formatting. macOS Storage uses decimal units (1 GB = 1,000,000,000 B),
//! so DiskDrift does the same to stay comparable.

pub fn format_bytes(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e12 {
        format!("{:.1} TB", b / 1e12)
    } else if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.1} KB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

/// Signed change, e.g. `+15.8 GB` / `-2.4 GB` / `0 B`.
pub fn format_delta(bytes: i64) -> String {
    if bytes > 0 {
        format!("+{}", format_bytes(bytes as u64))
    } else if bytes < 0 {
        format!("-{}", format_bytes(bytes.unsigned_abs()))
    } else {
        "0 B".to_string()
    }
}

/// Thousands separators for file counts: 143219 -> "143,219"
pub fn format_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1_500), "1.5 KB");
        assert_eq!(format_bytes(2_400_000_000), "2.4 GB");
    }

    #[test]
    fn deltas() {
        assert_eq!(format_delta(0), "0 B");
        assert_eq!(format_delta(15_800_000_000), "+15.8 GB");
        assert_eq!(format_delta(-2_400_000_000), "-2.4 GB");
    }

    #[test]
    fn counts() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1_000), "1,000");
        assert_eq!(format_count(143_219), "143,219");
        assert_eq!(format_count(1_234_567), "1,234,567");
    }
}
