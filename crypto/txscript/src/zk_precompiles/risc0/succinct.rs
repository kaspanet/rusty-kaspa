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

use std::collections::BTreeMap;

use alloc::{collections::VecDeque, string::String, vec::Vec};
use borsh::{BorshDeserialize, BorshSerialize};
use risc0_binfmt::{read_sha_halfs, Digestible};
use risc0_circuit_recursion::{control_id::ALLOWED_CONTROL_ROOT, CircuitImpl, CIRCUIT};
use risc0_core::field::baby_bear::BabyBearElem;
use risc0_zkp::core::hash::{blake2b::Blake2bCpuHashSuite, poseidon2::Poseidon2HashSuite, sha::Sha256HashSuite, HashSuite};
use risc0_zkp::{adapter::CircuitInfo, core::digest::Digest, verify::VerificationError};
use serde::{Deserialize, Serialize};

use crate::zk_precompiles::error::ZkIntegrityError;
use crate::zk_precompiles::risc0::merkle::MerkleProof;
use crate::zk_precompiles::risc0::sha;
/// A succinct receipt, produced via recursion, proving the execution of the zkVM with a [STARK].
///
/// Using recursion, a [CompositeReceipt][crate::CompositeReceipt] can be compressed to form a
/// [SuccinctReceipt]. In this way, a constant sized proof can be generated for arbitrarily long
/// computations, and with an arbitrary number of segments linked via composition.
///
/// [STARK]: https://dev.risczero.com/terminology#stark
#[derive(Debug, Serialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[non_exhaustive]
pub struct SuccinctReceipt {
    /// The cryptographic seal of this receipt. This seal is a STARK proving an execution of the
    /// recursion circuit.
    pub seal: Vec<u32>,

    /// The control ID of this receipt, identifying the recursion program that was run (e.g. lift,
    /// join, or resolve).
    pub control_id: Digest,

    /// Claim containing information about the computation that this receipt proves.
    ///
    /// The standard claim type is [ReceiptClaim][crate::ReceiptClaim], which represents a RISC-V
    /// zkVM execution.
    pub claim: Digest,

    /// Name of the hash function used to create this receipt.
    pub hashfn: String,

    /// A digest of the verifier parameters that can be used to verify this receipt.
    ///
    /// Acts as a fingerprint to identify differing proof system or circuit versions between a
    /// prover and a verifier. It is not intended to contain the full verifier parameters, which must
    /// be provided by a trusted source (e.g. packaged with the verifier code).
    pub verifier_parameters: Digest,

    /// Merkle inclusion proof for control_id against the control root for this receipt.
    pub control_inclusion_proof: MerkleProof,
}

impl SuccinctReceipt {
    /// Verify the integrity of this receipt, ensuring the claim is attested
    /// to by the seal.
    pub fn verify_integrity(&self) -> Result<(), ZkIntegrityError> {
        let suites: BTreeMap<String, HashSuite<risc0_zkp::field::baby_bear::BabyBear>> = BTreeMap::from([
            ("blake2b".into(), Blake2bCpuHashSuite::new_suite()),
            ("poseidon2".into(), Poseidon2HashSuite::new_suite()),
            ("sha-256".into(), Sha256HashSuite::new_suite()),
        ]);
        let suite = suites.get(&self.hashfn).ok_or(VerificationError::InvalidHashSuite)?;

        let check_code = |_, control_id: &Digest| -> Result<(), VerificationError> {
            self.control_inclusion_proof.verify(control_id, &ALLOWED_CONTROL_ROOT, suite.hashfn.as_ref()).map_err(|_| {
                tracing::debug!(
                    "failed to verify control inclusion proof for {control_id} against root {} with {}",
                    ALLOWED_CONTROL_ROOT,
                    suite.name,
                );
                VerificationError::ControlVerificationError { control_id: *control_id }
            })
        };

        // Verify the receipt itself is correct, and therefore the encoded globals are
        // reliable.
        risc0_zkp::verify::verify(&CIRCUIT, suite, &self.seal, check_code)?;

        // Extract the globals from the seal
        let output_elems: &[BabyBearElem] = bytemuck::checked::cast_slice(&self.seal[..CircuitImpl::OUTPUT_SIZE]);
        let mut seal_claim = VecDeque::new();
        for elem in output_elems {
            seal_claim.push_back(elem.as_u32())
        }

        // Read the Poseidon2 control root digest from the first 16 words of the output.
        // NOTE: Implemented recursion programs have two output slots, each of size 16 elems.
        // A SHA2 digest is encoded as 16 half words. Poseidon digests are encoded in 8 elems,
        // but are interspersed with padding to fill out the whole 16 elems.
        let control_root: Digest = seal_claim
            .drain(0..16)
            .enumerate()
            .filter_map(|(i, word)| (i & 1 == 0).then_some(word))
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| VerificationError::ReceiptFormatError)?;

        if control_root != ALLOWED_CONTROL_ROOT {
            tracing::debug!(
                "succinct receipt does not match the expected control root: decoded: {:#?}, expected: {:?}",
                control_root,
                ALLOWED_CONTROL_ROOT,
            );
            return Err(VerificationError::ControlVerificationError { control_id: control_root })?;
        }

        // Verify the output hash matches that data
        let output_hash = read_sha_halfs(&mut seal_claim).map_err(|_| VerificationError::ReceiptFormatError)?;
        if output_hash != self.claim {
            tracing::debug!(
                "succinct receipt claim does not match the output digest: claim: {:#?}, digest expected: {output_hash:?}",
                self.claim,
            );
            return Err(VerificationError::JournalDigestMismatch)?;
        }
        // Everything passed
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use risc0_binfmt::tagged_struct;
    use risc0_circuit_recursion::{control_id::ALLOWED_CONTROL_ROOT, CircuitImpl};
    use risc0_zkp::adapter::{CircuitInfo, PROOF_SYSTEM_INFO};
    use risc0_zkp::core::{
        digest::digest,
        hash::sha::{Impl, Sha256},
    };
    // Check that the verifier parameters has a stable digest (and therefore a stable value). This
    // struct encodes parameters used in verification, and so this value should be updated if and
    // only if a change to the verifier parameters is expected. Updating the verifier parameters
    // will result in incompatibility with previous versions.
    #[test]
    fn succinct_receipt_verifier_parameters_is_stable() {
        assert_eq!(
            tagged_struct::<Impl>(
                "risc0.SuccinctReceiptVerifierParameters",
                &[
                    ALLOWED_CONTROL_ROOT,
                    ALLOWED_CONTROL_ROOT,
                    *Impl::hash_bytes(&PROOF_SYSTEM_INFO.0),
                    *Impl::hash_bytes(&CircuitImpl::CIRCUIT_INFO.0),
                ],
                &[],
            ),
            digest!("08bfab58d6c29162aa18e69bc4cd7e109dc87fb7319072fb8a3d2131f149abb0")
        );
    }
}
