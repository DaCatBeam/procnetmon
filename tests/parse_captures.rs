//! Parses real `/proc/net/dev` captures, kept byte for byte in `tests/fixtures`.

use procnetmon::{model::Counter, parse::parse};
use rstest::rstest;

#[rstest]
fn parses_a_capture_from_linux_6_18() {
    let capture = include_str!("fixtures/linux-6.18.txt");
    let interfaces: Vec<_> = parse(capture)
        .collect::<Result<_, _>>()
        .expect("a real capture parses");
    let names: Vec<_> = interfaces.iter().map(|stats| stats.name).collect();
    assert_eq!(names, ["lo", "ifb0", "ifb1", "eth0"]);

    let eth0 = interfaces[3].counters;
    assert_eq!(eth0[Counter::RxBytes], 1_527_586);
    assert_eq!(eth0[Counter::RxPackets], 2_934);
    assert_eq!(eth0[Counter::RxDropped], 6);
    assert_eq!(eth0[Counter::TxBytes], 16_829_518);
    assert_eq!(eth0[Counter::TxPackets], 3_060);
}
