//! Reads this machine's own `/proc/net/dev`, so it only runs on Linux.
#![cfg(target_os = "linux")]

use procnetmon::{parse::parse, source::ProcNetMon};
use rstest::rstest;

#[rstest]
fn parses_this_machines_counters() {
    let mut source = ProcNetMon::open("/proc/net/dev").expect("Linux has /proc/net/dev");
    let contents = source.read().expect("it's readable");
    let interfaces: Vec<_> = parse(contents)
        .collect::<Result<_, _>>()
        .expect("the kernel's format parses");
    assert!(
        interfaces.iter().any(|stats| stats.name == "lo"),
        "every network namespace has a loopback interface"
    );
}
