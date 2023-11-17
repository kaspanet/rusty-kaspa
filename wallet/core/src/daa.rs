#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Daa(pub u64);

impl From<u64> for Daa {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<Daa> for u64 {
    fn from(value: Daa) -> Self {
        value.0
    }
}
