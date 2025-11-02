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

//! Manages the output and cryptographic data for a proven computation.
mod claim;
mod sha;
mod merkle;
pub mod groth16;
pub mod succinct;

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};

use borsh::{BorshDeserialize, BorshSerialize};
use risc0_core::field::baby_bear::BabyBear;
use risc0_zkp::{
    core::{
        digest::Digest,
        hash::{blake2b::Blake2bCpuHashSuite, poseidon2::Poseidon2HashSuite, sha::Sha256HashSuite, HashSuite},
    },
    verify::VerificationError,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

// Make succinct receipt available through this `receipt` module.
use claim::maybe_pruned::{MaybePruned, PrunedValueError};


// Make succinct receipt available through this `receipt` module.
pub use self::groth16::{Groth16Receipt, Groth16ReceiptVerifierParameters};
use claim::Unknown;
use crate::zk_precompiles::risc0::sha::{Digestible, Sha256};

pub use self::succinct::{SuccinctReceipt, SuccinctReceiptVerifierParameters};


/// A record of the public commitments for a proven zkVM execution.
///
/// Public outputs, including commitments to important inputs, are written to the journal during
/// zkVM execution. Along with an image ID, it constitutes the statement proven by a given
/// [Receipt]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Journal {
    /// The raw bytes of the journal.
    pub bytes: Vec<u8>,
}

impl Journal {
    /// Construct a new [Journal].
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

}

impl risc0_binfmt::Digestible for Journal {
    fn digest<S: Sha256>(&self) -> Digest {
        *S::hash_bytes(&self.bytes)
    }
}

impl AsRef<[u8]> for Journal {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}


/// Metadata providing context on the receipt.
///
/// It contains information about the proving system, SDK versions, and other information to help
/// with interoperability. It is not cryptographically bound to the receipt, and should not be used
/// for security-relevant decisions, such as choosing whether or not to accept a receipt based on
/// it's stated version.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, BorshSerialize, BorshDeserialize)]
#[non_exhaustive]
pub struct ReceiptMetadata {
    /// Information which can be used to decide whether a given verifier is compatible with this
    /// receipt (i.e. that it may be able to verify it).
    ///
    /// It is intended to be used when there are multiple verifier implementations (e.g.
    /// corresponding to multiple versions of a proof system or circuit) and it is ambiguous which
    /// one should be used to attempt verification of a receipt.
    pub verifier_parameters: Digest,
}


/// Maximum segment size, as a power of two (po2) that the default verifier parameters will accept.
///
/// A default of 21 was selected to reach a target of 97 bits of security under our analysis. Using
/// a po2 higher than 21 shows a degradation of 1 bit of security per po2, to 94 bits at po2 24.
pub const DEFAULT_MAX_PO2: usize = 22;

/// Context available to the verification process.
#[non_exhaustive]
pub struct VerifierContext {
    /// A registry of hash functions to be used by the verification process.
    pub suites: BTreeMap<String, HashSuite<BabyBear>>,

    /// Parameters for verification of [SuccinctReceipt].
    pub succinct_verifier_parameters: Option<SuccinctReceiptVerifierParameters>,

    /// Parameters for verification of [Groth16Receipt].
    pub groth16_verifier_parameters: Option<Groth16ReceiptVerifierParameters>,

}

impl VerifierContext {
    /// Create an empty [VerifierContext].
    pub fn empty() -> Self {
        Self {
            suites: BTreeMap::default(),
            succinct_verifier_parameters: None,
            groth16_verifier_parameters: None,
        }
    }

    /// Return the mapping of hash suites used in the default [VerifierContext].
    pub fn default_hash_suites() -> BTreeMap<String, HashSuite<BabyBear>> {
        BTreeMap::from([
            ("blake2b".into(), Blake2bCpuHashSuite::new_suite()),
            ("poseidon2".into(), Poseidon2HashSuite::new_suite()),
            ("sha-256".into(), Sha256HashSuite::new_suite()),
        ])
    }


    /// Return [VerifierContext] with the given map of hash suites.
    pub fn with_suites(mut self, suites: BTreeMap<String, HashSuite<BabyBear>>) -> Self {
        self.suites = suites;
        self
    }
    /// Return [VerifierContext] with the given [SuccinctReceiptVerifierParameters] set.
    pub fn with_succinct_verifier_parameters(mut self, params: SuccinctReceiptVerifierParameters) -> Self {
        self.succinct_verifier_parameters = Some(params);
        self
    }

    /// Return [VerifierContext] with the given [Groth16ReceiptVerifierParameters] set.
    pub fn with_groth16_verifier_parameters(mut self, params: Groth16ReceiptVerifierParameters) -> Self {
        self.groth16_verifier_parameters = Some(params);
        self
    }


}

impl Default for VerifierContext {
    fn default() -> Self {
        Self {
            suites: Self::default_hash_suites(),
            succinct_verifier_parameters: Some(Default::default()),
            groth16_verifier_parameters: Some(Default::default()),
        }
    }
}
