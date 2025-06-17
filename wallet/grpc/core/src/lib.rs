pub mod kaspawalletd {
    include!(concat!(env!("OUT_DIR"), "/kaspawalletd.rs"));
}

pub mod protoserialization {
    include!(concat!(env!("OUT_DIR"), "/protoserialization.rs"));

    impl PartiallySignedTransaction {
        pub fn encode_to_vec(&self) -> Vec<u8> {
            prost::Message::encode_to_vec(self)
        }
    }
}
pub mod convert;
