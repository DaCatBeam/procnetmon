//! Runs the built binary the way a user would.

use std::process::{Command, Output};

use rstest::rstest;

fn procnetmon(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_procnetmon"))
        .args(args)
        .output()
        .expect("the binary runs")
}

#[rstest]
fn a_missing_counters_file_fails_and_says_which() {
    let output = procnetmon(&["--file", "/no/such/counters"]);
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("could not open /no/such/counters"),
        "{stderr}"
    );
}

#[rstest]
#[case::interval(&["--poll-interval", "10"])]
#[case::window(&["--window", "5"])]
fn out_of_range_options_are_usage_errors(#[case] args: &[&str]) {
    let output = procnetmon(args);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

#[rstest]
fn prints_its_version() {
    let output = procnetmon(&["--version"]);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        concat!("procnetmon ", env!("CARGO_PKG_VERSION"))
    );
}
