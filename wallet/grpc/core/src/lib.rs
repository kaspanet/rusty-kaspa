pub mod kaspawalletd {
    include!(concat!(env!("OUT_DIR"), "/kaspawalletd.rs"));
}

pub mod protoserialization {
    include!(concat!(env!("OUT_DIR"), "/protoserialization.rs"));
}
pub mod convert;
