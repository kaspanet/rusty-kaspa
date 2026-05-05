use std::marker::PhantomData;

use super::super::Result;
use crate::zk_precompiles::{
    risc0::zk_to_script::{R0ScriptBuilder, UnboundedZkScript, UninitializedZkScript},
    tags::ZkTag,
};
