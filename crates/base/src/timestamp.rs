#[derive(Debug, Default, Ord, PartialOrd, Eq, PartialEq, Clone, Copy, Hash)]
pub struct Timestamp {
    pub nanoseconds: u128,
}

impl Timestamp {
    pub fn new(nanoseconds: u128) -> Self {
        Self { nanoseconds }
    }

    pub fn elapsed_milliseconds_since(&self, start: &Timestamp) -> u64 {
        ((self.nanoseconds - start.nanoseconds) / 1_000_000) as u64
    }
}
