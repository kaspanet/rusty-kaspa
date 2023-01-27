pub enum AddressType {
    Receive = 0,
    Change,
}

impl ToString for AddressType {
    fn to_string(&self) -> String {
        match self {
            Self::Receive => "Receive",
            Self::Change => "Change",
        }
        .to_string()
    }
}

impl AddressType {
    pub fn index(&self) -> u32 {
        match self {
            Self::Receive => 0,
            Self::Change => 1,
        }
    }
}
