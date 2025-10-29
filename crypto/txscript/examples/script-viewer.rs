use kaspa_consensus_core::{hashing::sighash::SigHashReusedValuesSync, tx::ValidatedTransaction};
use kaspa_txscript::script_builder::ScriptBuilder;

fn main() {
    let script_vec = hex::decode("4130ef124590e4e6627078a658e2eb0b89fe4733f40d8cbfe0d077ae16bb90afb0a5f10e5693352e4b9d19d77a98fe75e395ce60988a0750ab8603a252c9c7290401412294d292317d03d1a5f49a8204c35486da84bbbea604209637e2bbfb5bbfabb36bcb37fc90aeb9836ed950a42b87382880fbd926b362cdbca16e9db9891918850141c2d76d4c64c9b8a8a64fa34a69f7cea953c4f0e564463226d931481ee1fbccafd7c20500a699fc8a10d01d03219d25944081750cdbba89e6a5a64b3224f58a5a014c875320b0a2f302b97271d6d1f20f2168e8b86b037d42a52aaf7ca959bea8a8bbf859a220e040996f44024491881ad4d2f59d4397a5a1f2e169c55624cb9509693fbb7a14204e518f0ecb51eef7db45042e441bb4d99f2c68277359bea369fcb7c80bee5b0120924013135715c9a8076141a33d6528a13fa2e816d3f006897b6d6c8b1da90fd754ae").unwrap();

    // build the script from hex
    let mut s = ScriptBuilder::new();
    s.script_mut().extend_from_slice(&script_vec);

    // print the hexadecimal form
    println!("{}", s.hex_view(0, 30));

    // print the human readable form
    println!("{}", s.string_view::<ValidatedTransaction, SigHashReusedValuesSync>());
}
