//! Human-readable numbers for chart labels.

use crate::history::as_f64;

/// Binary (IEC) units, so 1 KiB is 1024 B. Each includes the space before it.
const BINARY: [&str; 7] = [" B", " KiB", " MiB", " GiB", " TiB", " PiB", " EiB"];

/// SI prefixes for plain counts. A count under 1000 has no unit, so the first is empty.
const SI: [&str; 7] = ["", " k", " M", " G", " T", " P", " E"];

/// Formats a byte rate with binary (IEC) units, for example `"1.5 MiB/s"`.
///
/// Below 1 KiB/s it prints whole bytes (`"512 B/s"`), and from there one decimal place. The unit
/// steps up before the number would round to 1024, so it never prints `"1024 B/s"`. Anything that
/// isn't a positive, finite number, like a negative or NaN rate, prints as `"0 B/s"`.
#[must_use]
pub fn rate(bytes_per_sec: f64) -> String {
    scaled(bytes_per_sec, 1024.0, &BINARY, "/s")
}

/// Formats a byte count the same way, for example `"3.2 GiB"`.
#[must_use]
pub fn bytes(n: u64) -> String {
    scaled(as_f64(n), 1024.0, &BINARY, "")
}

/// Formats a plain count with SI prefixes, for example `"999"` or `"12.3 k"`.
#[must_use]
pub fn count(n: u64) -> String {
    scaled(as_f64(n), 1000.0, &SI, "")
}

/// `value` in the largest of `units`, each `base` times the one before, that keeps the number
/// below `base`, followed by `suffix`.
fn scaled(value: f64, base: f64, units: &[&str], suffix: &str) -> String {
    let value = if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    };
    // From `base - 0.5` up, a whole number would round to `base`.
    if value < base - 0.5 {
        return format!("{value:.0}{}{suffix}", units[0]);
    }
    let mut value = value / base;
    let mut unit = 1;
    // Likewise, from `base - 0.05` up, one decimal place would round to `base`.
    while value >= base - 0.05 && unit + 1 < units.len() {
        value /= base;
        unit += 1;
    }
    format!("{value:.1}{}{suffix}", units[unit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::idle(0.0, "0 B/s")]
    #[case::whole_bytes(512.4, "512 B/s")]
    #[case::just_below_a_kib(1023.4, "1023 B/s")]
    #[case::rounds_up_to_a_kib(1023.5, "1.0 KiB/s")]
    #[case::one_decimal(1536.0, "1.5 KiB/s")]
    #[case::just_below_a_mib(1023.94 * 1024.0, "1023.9 KiB/s")]
    #[case::rounds_up_to_a_mib(1023.95 * 1024.0, "1.0 MiB/s")]
    #[case::ten_gigabit_line_rate(1.25e9, "1.2 GiB/s")]
    #[case::stops_at_eib(2.0 * 1024_f64.powi(7), "2048.0 EiB/s")]
    #[case::negative(-1.0, "0 B/s")]
    #[case::negative_zero(-0.0, "0 B/s")]
    #[case::nan(f64::NAN, "0 B/s")]
    #[case::infinite(f64::INFINITY, "0 B/s")]
    fn rate_uses_binary_units(#[case] bytes_per_sec: f64, #[case] expected: &str) {
        assert_eq!(rate(bytes_per_sec), expected);
    }

    #[rstest]
    #[case::zero(0, "0 B")]
    #[case::whole_bytes(1023, "1023 B")]
    #[case::one_decimal(1536, "1.5 KiB")]
    #[case::gib((3 << 30) + (200 << 20), "3.2 GiB")]
    #[case::largest(u64::MAX, "16.0 EiB")]
    fn bytes_use_binary_units(#[case] n: u64, #[case] expected: &str) {
        assert_eq!(bytes(n), expected);
    }

    #[rstest]
    #[case::zero(0, "0")]
    #[case::below_a_thousand(999, "999")]
    #[case::a_thousand(1_000, "1.0 k")]
    #[case::one_decimal(12_345, "12.3 k")]
    #[case::just_below_a_million(999_949, "999.9 k")]
    #[case::rounds_up_to_a_million(999_950, "1.0 M")]
    #[case::largest(u64::MAX, "18.4 E")]
    fn counts_use_si_prefixes(#[case] n: u64, #[case] expected: &str) {
        assert_eq!(count(n), expected);
    }
}
