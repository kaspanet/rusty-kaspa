// use crate::role::*;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

#[derive(Debug, Serialize, Deserialize)]
pub struct Bundle<ROLE> {
    #[serde(skip_serializing, default)]
    role: PhantomData<ROLE>,
}
