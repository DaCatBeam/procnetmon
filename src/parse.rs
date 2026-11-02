use crate::model::{Counter, Counters, InterfaceStats};
use std::num::ParseIntError;

const HEADER_PREFIXES: [&str; 2] = ["Inter-|", " face |"];

pub fn parse(contents: &str) -> impl Iterator<Item = Result<InterfaceStats<'_>, ParseError>> {
    let mut lines = contents.lines().enumerate();
    let bad_header = HEADER_PREFIXES
        .iter()
        .find_map(|prefix| match lines.next() {
            Some((idx, text)) if !text.starts_with(prefix) => {
                Some(ParseError::UnexpectedHeader { line: idx + 1 })
            }
            _ => None,
        });

    let body_lines = if bad_header.is_some() { 0 } else { usize::MAX };
    bad_header.map(Err).into_iter().chain(
        lines
            .take(body_lines)
            .filter(|(_, text)| !text.trim().is_empty())
            .map(|(idx, text)| parse_line(idx + 1, text)),
    )
}

fn parse_line(line: usize, text: &str) -> Result<InterfaceStats<'_>, ParseError> {
    let (name, rest) = text
        .split_once(':')
        .ok_or(ParseError::MissingColon { line })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(ParseError::EmptyName { line });
    }

    let mut fields = rest.split_ascii_whitespace();
    let mut counters = Counters::default();
    for (found, counter) in Counter::ALL.into_iter().enumerate() {
        let raw = fields
            .next()
            .ok_or(ParseError::FieldCount { line, found })?;

        counters[counter] = raw.parse().map_err(|source| ParseError::InvalidNumber {
            line,
            counter,
            source,
        })?;
    }

    let extra = fields.count();
    if extra > 0 {
        return Err(ParseError::FieldCount {
            line,
            found: Counter::COUNT + extra,
        });
    }

    Ok(InterfaceStats { name, counters })
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum ParseError {
    #[error("line {line}: expected `<interface>: <counters>`")]
    MissingColon { line: usize },
    #[error("line {line}: empty interface name")]
    EmptyName { line: usize },
    #[error("line {line}: expected 16 counters, found {found}")]
    FieldCount { line: usize, found: usize },
    #[error("line {line}: invalid value for {}", .counter.label())]
    InvalidNumber {
        line: usize,
        counter: Counter,
        #[source]
        source: ParseIntError,
    },
    #[error("line {line}: not a /proc/net/dev header")]
    UnexpectedHeader { line: usize },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::{fixture, rstest};
    use std::ops::Index;

    static SIMPLE_PARSING_INTERFACE: &[&str] = &[
        "lo",
        "tunl0",
        "gre0",
        "gretap0",
        "erspan0",
        "ip_vti0",
        "ip6_vti0",
        "sit0",
        "ip6tnl0",
        "ip6gre0",
        "eth0",
        "br-3f2a1b4c5d6e",
        "eth0.100",
        "flannel.1",
    ];

    const HEADER: &str = indoc::indoc! {"
        Inter-|   Receive                                                |  Transmit
         face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    "};

    const ZEROS: &str = "0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";

    fn with_header(body: &str) -> String {
        format!("{HEADER}{body}")
    }

    fn parse_ok(input: &str) -> Vec<InterfaceStats<'_>> {
        parse(input)
            .collect::<Result<_, _>>()
            .expect("input should parse")
    }

    #[rstest]
    fn parses_every_interface_in_file_order() {
        let input = with_header(&format!(
            "    lo: {ZEROS}\n  eth0: {ZEROS}\nbr-3f2a1b4c5d6e: {ZEROS}\neth0.100: {ZEROS}\nflannel.1: {ZEROS}\n"
        ));
        let names: Vec<_> = parse_ok(&input).iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            ["lo", "eth0", "br-3f2a1b4c5d6e", "eth0.100", "flannel.1"]
        );
    }

    #[rstest]
    #[case::missing_colon("eth0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0", ParseError::MissingColon { line: 3 })]
    #[case::empty_name(": 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0", ParseError::EmptyName { line: 3 })]
    #[case::too_few_fields("eth0: 1 2 3", ParseError::FieldCount { line: 3, found: 3 })]
    #[case::too_many_fields("eth0: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 17", ParseError::FieldCount { line: 3, found: 17 })]
    fn reports_malformed_lines(#[case] line: &str, #[case] expected: ParseError) {
        let input = with_header(&format!("{line}\n"));
        assert_eq!(parse(&input).next(), Some(Err(expected)));
    }

    #[rstest]
    fn rejects_inputs_without_kernel_header() {
        let input = &format!("    lo: {ZEROS}\n");

        assert_eq!(
            parse(input).next(),
            Some(Err(ParseError::UnexpectedHeader { line: 1 }))
        );
    }

    #[rstest]
    fn filters_empty_lines() {
        let input = with_header(&format!("    lo: {ZEROS}\n\neth0:{ZEROS}"));

        let stats = parse_ok(&input);

        assert_eq!(stats.len(), 2);
    }

    const KERNEL_COLUMNS: [(&str, Counter, &str); Counter::COUNT] = [
        ("bytes", Counter::RxBytes, "rx_bytes"),
        ("packets", Counter::RxPackets, "rx_packets"),
        ("errs", Counter::RxErrors, "rx_errors"),
        ("drop", Counter::RxDropped, "rx_dropped"),
        ("fifo", Counter::RxFifo, "rx_fifo"),
        ("frame", Counter::RxFrame, "rx_frame"),
        ("compressed", Counter::RxCompressed, "rx_compressed"),
        ("multicast", Counter::RxMulticast, "rx_multicast"),
        ("bytes", Counter::TxBytes, "tx_bytes"),
        ("packets", Counter::TxPackets, "tx_packets"),
        ("errs", Counter::TxErrors, "tx_errors"),
        ("drop", Counter::TxDropped, "tx_dropped"),
        ("fifo", Counter::TxFifo, "tx_fifo"),
        ("colls", Counter::TxCollisions, "tx_collisions"),
        ("carrier", Counter::TxCarrier, "tx_carrier"),
        ("compressed", Counter::TxCompressed, "tx_compressed"),
    ];

    #[rstest]
    fn columns_land_in_the_right_counters() {
        // The table matches the header the kernel actually prints...
        let header_names: Vec<&str> = HEADER
            .lines()
            .nth(1)
            .unwrap()
            .split('|')
            .skip(1)
            .flat_map(str::split_whitespace)
            .collect();
        let table_names: Vec<&str> = KERNEL_COLUMNS.iter().map(|&(name, ..)| name).collect();
        assert_eq!(header_names, table_names);

        // ...and each column's value lands in the counter the table names.
        let input = with_header("eth0: 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15\n");
        let stats = parse_ok(&input)[0];
        for (i, &(column, counter, label)) in KERNEL_COLUMNS.iter().enumerate() {
            let expected = u64::try_from(i).unwrap();
            assert_eq!(
                stats.counters[counter], expected,
                "column {i} ({column}) should be {counter:?}"
            );
            assert_eq!(counter.label(), label);
        }
    }

    #[fixture]
    fn handles_simple_parsing_fixture() -> &'static str {
        indoc::indoc! {"
            Inter-|   Receive                                                |  Transmit
             face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
                lo:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
             tunl0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
              gre0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
            gretap0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
            erspan0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
            ip_vti0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
            ip6_vti0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
              sit0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
            ip6tnl0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
            ip6gre0:       0       0    0    0    0     0          0         0        0       0    0    0    0     0       0          0
              eth0:   0      1    2    3    4     5          6         7     8      9    10    11    12     13       14          15
              br-3f2a1b4c5d6e: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
              eth0.100: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
              flannel.1: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
        "}
    }

    #[rstest]
    fn handles_simple_parsing(handles_simple_parsing_fixture: &'static str) {
        let iterator = parse(handles_simple_parsing_fixture);
        let call_result: Vec<_> = iterator.collect();

        // Finds all 14 interfaces are described in the file
        assert_eq!(call_result.len(), 14);

        // All results in the returned iterator are Ok
        assert!(call_result.iter().all(Result::is_ok));

        let unwrapped_items: Vec<&InterfaceStats<'_>> =
            call_result.iter().map(|r| r.as_ref().unwrap()).collect();

        // That we capture bridges and dotted interfaces as well as all interfaces are kept in order
        for (idx, item) in unwrapped_items.iter().enumerate() {
            assert_eq!(item.name, SIMPLE_PARSING_INTERFACE[idx]);
        }

        let eth0_interface_stats = unwrapped_items.get(10).unwrap();

        // That we collected eth0
        assert_eq!(eth0_interface_stats.name, "eth0");

        // For each eth0 statistic, assert the correct value has been captured.
        for idx in 0..Counter::COUNT {
            assert_eq!(eth0_interface_stats.counters.0.index(idx), &(idx as u64));
        }
    }

    #[rstest]
    fn keeps_full_u64_precision() {
        let beyond_f64 = (1_u64 << 53) + 1;
        let input = with_header(&format!(
            "eth0: {} 0 0 0 0 0 0 0 {beyond_f64} 0 0 0 0 0 0 0\n",
            u64::MAX
        ));
        let stats = parse_ok(&input)[0];
        assert_eq!(stats.counters[Counter::RxBytes], u64::MAX);
        assert_eq!(stats.counters[Counter::TxBytes], beyond_f64);
    }

    #[fixture]
    fn returns_empty_with_headers_only_fixture() -> &'static str {
        indoc::indoc! {"
            Inter-|   Receive                                                |  Transmit
             face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
        "}
    }

    #[rstest]
    fn returns_empty_with_headers_only(returns_empty_with_headers_only_fixture: &'static str) {
        let iterator = parse(returns_empty_with_headers_only_fixture);

        // That we received no interface stats
        assert_eq!(iterator.count(), 0);
    }
}
