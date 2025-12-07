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

use borsh::{BorshDeserialize, BorshSerialize};
use risc0_binfmt::{tagged_struct, Digestible, ExitCode, SystemState};
use risc0_zkp::core::{digest::Digest, hash::sha::Sha256};
use serde::{Deserialize, Serialize};

use crate::zk_precompiles::error::ZkIntegrityError;

/// Public claims about a zkVM guest execution, such as the journal committed to by the guest.
///
/// Also includes important information such as the exit code and the starting and ending system
/// state (i.e. the state of memory). [ReceiptClaim] is a "Merkle-ized struct" supporting
/// partial openings of the underlying fields from a hash commitment to the full structure. Also
/// see [MaybePruned].
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(test, derive(PartialEq))]
struct ReceiptClaim {
    /// The [SystemState] just before execution has begun.
    pub pre: Digest,

    /// The [SystemState] just after execution has completed.
    pub post: SystemState,

    /// The exit code for the execution.
    pub exit_code: ExitCode,

    /// Input to the guest.
    pub input: Digest,

    /// [Output] of the guest, including the journal and assumptions set during execution.
    pub output: Output,
}

/// Construct a [ReceiptClaim] representing a zkVM execution that ended normally (i.e.
/// Halted(0)) with the given image ID and journal.
pub fn compute_assert_claim(claim: &Digest, image_id: Digest, journal_hash: Digest) -> Result<(), ZkIntegrityError> {
    let computed_claim = ReceiptClaim {
        pre: image_id,
        post: SystemState { pc: 0, merkle_root: Digest::ZERO },
        exit_code: ExitCode::Halted(0),
        input: Digest::ZERO,
        output: Output { journal: journal_hash, assumptions: Digest::ZERO },
    }
    .digest::<risc0_zkp::core::hash::sha::Impl>();

    // If the claim does not match the computed claim, return an error
    if *claim != computed_claim {
        return Err(ZkIntegrityError::R0Verification(format!(
            "Claim: {:?} does not match the computed claim digest: {:?}",
            claim, computed_claim
        )));
    }
    Ok(())
}

impl Digestible for ReceiptClaim {
    /// Hash the [ReceiptClaim] to get a digest of the struct.
    fn digest<S: Sha256>(&self) -> Digest {
        let (sys_exit, user_exit) = self.exit_code.into_pair();
        tagged_struct::<S>(
            "risc0.ReceiptClaim",
            &[self.input, self.pre, self.post.digest::<S>(), self.output.digest::<S>()],
            &[sys_exit, user_exit],
        )
    }
}

/// Output field in the [ReceiptClaim], committing to a claimed journal and assumptions list.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Output {
    /// The journal committed to by the guest execution.
    pub journal: Digest,

    /// An ordered list of [ReceiptClaim] digests corresponding to the
    /// calls to `env::verify` and `env::verify_integrity`.
    ///
    /// Verifying the integrity of a [crate::Receipt] corresponding to a [ReceiptClaim] with a
    /// non-empty assumptions list does not guarantee unconditionally any of the claims over the
    /// guest execution (i.e. if the assumptions list is non-empty, then the journal digest cannot
    /// be trusted to correspond to a genuine execution). The claims can be checked by additional
    /// verifying a [crate::Receipt] for every digest in the assumptions list.
    pub assumptions: Digest,
}

impl Digestible for Output {
    /// Hash the [Output] to get a digest of the struct.
    fn digest<S: Sha256>(&self) -> Digest {
        tagged_struct::<S>("risc0.Output", &[self.journal, self.assumptions], &[])
    }
}
