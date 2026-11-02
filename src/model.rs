use std::ops::{Index, IndexMut};

/// Unit determines how a counter is charted. Counters of the same kind can share an axis.
/// For instance, if `FooBytes` and `BarBytes` are of type `Unit::Byte`, they may be represented
/// by the same axis on a chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    /// Represents bytes per sec
    Byte,
    /// Represents event counts, such as error events, per sec
    Event,
    /// Packets per sec: packets, compressed, multicast.
    Packet,
}

/// Counter represents the statistics columns, in order, to be collected out of /proc/net/dev
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Counter {
    RxBytes,
    RxPackets,
    RxErrors,
    RxDropped,
    RxFifo,
    RxFrame,
    RxCompressed,
    RxMulticast,
    TxBytes,
    TxPackets,
    TxErrors,
    TxDropped,
    TxFifo,
    TxCollisions,
    TxCarrier,
    TxCompressed,
}

impl Counter {
    /// The number of statistics in each row of /proc/net/dev
    pub const COUNT: usize = 16;

    /// Represents the order of statistics columns in /proc/net/dev
    pub const ALL: [Self; Self::COUNT] = [
        Counter::RxBytes,
        Counter::RxPackets,
        Counter::RxErrors,
        Counter::RxDropped,
        Counter::RxFifo,
        Counter::RxFrame,
        Counter::RxCompressed,
        Counter::RxMulticast,
        Counter::TxBytes,
        Counter::TxPackets,
        Counter::TxErrors,
        Counter::TxDropped,
        Counter::TxFifo,
        Counter::TxCollisions,
        Counter::TxCarrier,
        Counter::TxCompressed,
    ];

    /// Represents the index of the Counter within `Counter::ALL`
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Provides a string label for human readability
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Counter::RxBytes => "rx_bytes",
            Counter::RxPackets => "rx_packets",
            Counter::RxErrors => "rx_errors",
            Counter::RxDropped => "rx_dropped",
            Counter::RxFifo => "rx_fifo",
            Counter::RxFrame => "rx_frame",
            Counter::RxCompressed => "rx_compressed",
            Counter::RxMulticast => "rx_multicast",
            Counter::TxBytes => "tx_bytes",
            Counter::TxPackets => "tx_packets",
            Counter::TxErrors => "tx_errors",
            Counter::TxDropped => "tx_dropped",
            Counter::TxFifo => "tx_fifo",
            Counter::TxCollisions => "tx_collisions",
            Counter::TxCarrier => "tx_carrier",
            Counter::TxCompressed => "tx_compressed",
        }
    }

    /// Provides unit, which informs as to which Counter types may be charted or represented together upon an axis.
    #[must_use]
    pub const fn unit(self) -> Unit {
        match self {
            Counter::RxBytes | Counter::TxBytes => Unit::Byte,
            Counter::RxPackets
            | Counter::TxPackets
            | Counter::RxDropped
            | Counter::TxDropped
            | Counter::RxCompressed
            | Counter::TxCompressed
            | Counter::RxMulticast => Unit::Packet,
            Counter::RxErrors
            | Counter::RxFifo
            | Counter::RxFrame
            | Counter::TxErrors
            | Counter::TxFifo
            | Counter::TxCollisions
            | Counter::TxCarrier => Unit::Event,
        }
    }
}

/// Fixed-size array representing the counters of a single /proc/net/dev interface line
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counters(pub [u64; Counter::COUNT]);

impl Index<Counter> for Counters {
    type Output = u64;
    fn index(&self, counter: Counter) -> &Self::Output {
        &self.0[counter.index()]
    }
}

impl IndexMut<Counter> for Counters {
    fn index_mut(&mut self, counter: Counter) -> &mut Self::Output {
        &mut self.0[counter.index()]
    }
}

/// Fully represents an interface line from /proc/net/dev, (name and statistics)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceStats<'a> {
    pub name: &'a str,
    pub counters: Counters,
}

/// Whether `Counter::ALL` lists the variants in declaration order, which indexing relies on.
#[mutants::skip] // It only runs at compile time, where a mutant can only switch the check off.
const fn all_is_in_declaration_order() -> bool {
    let mut i = 0;
    while i < Counter::COUNT {
        if Counter::ALL[i] as usize != i {
            return false;
        }
        i += 1;
    }
    true
}

// Compile time guard to ensure order is preserved.
const _: () = assert!(
    all_is_in_declaration_order(),
    "Counter::ALL must list variants in declaration order"
);

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::collections::HashSet;

    #[rstest]
    fn labels_are_unique_snake_case_and_say_their_direction() {
        let unique: HashSet<_> = Counter::ALL.iter().map(|c| c.label()).collect();
        assert_eq!(unique.len(), Counter::COUNT, "labels must be unique");

        for counter in Counter::ALL {
            let label = counter.label();
            // The first eight columns are receive counters, the last eight transmit.
            let prefix = if counter.index() < 8 { "rx_" } else { "tx_" };
            assert!(label.starts_with(prefix), "{counter:?} -> {label}");
            assert!(
                label.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
                "{label} isn't snake_case"
            );
        }
    }

    #[rstest]
    fn units_match_what_each_column_counts() {
        for counter in [Counter::RxBytes, Counter::TxBytes] {
            assert_eq!(counter.unit(), Unit::Byte, "{counter:?}");
        }
        for counter in [
            Counter::RxPackets,
            Counter::TxPackets,
            Counter::RxDropped,
            Counter::TxDropped,
            Counter::RxCompressed,
            Counter::TxCompressed,
            Counter::RxMulticast,
        ] {
            assert_eq!(counter.unit(), Unit::Packet, "{counter:?}");
        }
    }

    #[rstest]
    fn indexing_by_counter_reads_and_writes_its_column() {
        let mut counters = Counters::default();
        counters[Counter::TxBytes] = 42;
        assert_eq!(counters.0[8], 42);
        assert_eq!(counters[Counter::TxBytes], 42);
    }
}
