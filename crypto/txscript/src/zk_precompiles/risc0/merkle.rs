// Copyright 2025 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal Merkle tree implementation used in the recursion system for
//! committing to a group of control IDs.

use alloc::vec::Vec;

use borsh::{BorshDeserialize, BorshSerialize};
use risc0_core::field::baby_bear::BabyBear;
use risc0_zkp::core::{digest::Digest, hash::HashFn};
use serde::{Deserialize, Serialize};

use crate::zk_precompiles::risc0::R0Error;

/// An inclusion proof for the [MerkleGroup]. Used to verify inclusion of a
/// given recursion program in the committed set.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct MerkleProof {
    /// Index of the leaf for which inclusion is being proven.
    pub index: u32,

    /// Sibling digests on the path from the root to the leaf.
    /// Does not include the root of the leaf.
    pub digests: Vec<Digest>,
}

impl MerkleProof {
    /// Verify the Merkle inclusion proof against the given leaf and root.
    pub fn verify(&self, leaf: &Digest, root: &Digest, hashfn: &dyn HashFn<BabyBear>) -> Result<(), R0Error> {
        if self.root(leaf, hashfn) == *root {
            return Ok(());
        }
        Err(R0Error::Merkle)
    }

    /// Calculate the root of this branch by iteratively hashing, starting from the leaf.
    pub fn root(&self, leaf: &Digest, hashfn: &dyn HashFn<BabyBear>) -> Digest {
        let mut cur = *leaf;
        let mut cur_index = self.index;
        for sibling in &self.digests {
            cur = if cur_index & 1 == 0 { *hashfn.hash_pair(&cur, sibling) } else { *hashfn.hash_pair(sibling, &cur) };
            cur_index >>= 1;
        }
        cur
    }
}
