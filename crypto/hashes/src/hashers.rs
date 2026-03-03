// use sha3::CShake256;

pub trait HasherBase {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self;
}

pub trait Hasher: HasherBase + Clone + Default {
    fn finalize(self) -> crate::Hash;
    fn reset(&mut self);
    #[inline(always)]
    fn hash<A: AsRef<[u8]>>(data: A) -> crate::Hash {
        let mut hasher = Self::default();
        hasher.update(data);
        hasher.finalize()
    }
}

// Implemented manually in pow_hashers:
//  struct PowHash => `cSHAKE256("ProofOfWorkHash")
//  struct KHeavyHash => `cSHAKE256("HeavyHash")
pub use crate::pow_hashers::{KHeavyHash, PowHash};
blake2b_hasher! {
    struct TransactionHash => b"TransactionHash",
    struct TransactionID => b"TransactionID",
    struct TransactionSigningHash => b"TransactionSigningHash",
    struct BlockHash => b"BlockHash",
    struct ProofOfWorkHash => b"ProofOfWorkHash",
    struct MerkleBranchHash => b"MerkleBranchHash",
    struct MuHashElementHash => b"MuHashElement",
    struct MuHashFinalizeHash => b"MuHashFinalize",
    struct PersonalMessageSigningHash => b"PersonalMessageSigningHash",
    struct CovenantID => b"CovenantID",
}

sha256_hasher! {
    struct TransactionSigningHashECDSA => b"TransactionSigningHashECDSA",
}

blake3_hasher! {
    struct SeqCommitmentMerkleBranchHash => b"SeqCommitmentMerkleBranchHash",
    struct SeqCommitmentMerkleLeafHash => b"SeqCommitmentMerkleLeafHash",
    struct PayloadDigest => b"PayloadDigest",
    struct TransactionRest => b"TransactionRest",
    struct TransactionV1Id => b"TransactionV1Id",
    struct SeqCommitLaneKey => b"SeqCommitLaneKey",
    struct SeqCommitLaneTip => b"SeqCommitLaneTip",
    struct SeqCommitActivityLeaf => b"SeqCommitActivityLeaf",
    struct SeqCommitMergesetContext => b"SeqCommitMergesetContext",
    struct SeqCommitMinerPayload => b"SeqCommitMinerPayload",
    struct SeqCommitMinerPayloadLeaf => b"SeqCommitMinerPayloadLeaf",
    struct SeqCommitActiveLeaf => b"SeqCommitActiveLeaf",
    struct SeqCommitActiveNode => b"SeqCommitActiveNode",
}

#[macro_export]
macro_rules! sha256_hasher {
    ($(struct $name:ident => $domain_sep:literal),+ $(,)? ) => {$(
        #[derive(Clone)]
        pub struct $name($crate::sha2::Sha256);

        impl $name {
            #[inline]
            pub fn new() -> Self {
                use $crate::sha2::{Digest as _};
                const DOMAIN_HASH: [u8; 32] =  {
                    $crate::sha2_const_stable::Sha256::new().update($domain_sep).finalize()
                };
                Self($crate::sha2::Sha256::new_with_prefix(DOMAIN_HASH))
            }

            pub fn write<A: AsRef<[u8]>>(&mut self, data: A) {
                $crate::sha2::Digest::update(&mut self.0, data.as_ref());
            }

            #[inline(always)]
            pub fn finalize(self) -> $crate::Hash {
                let mut out = [0u8; 32];
                out.copy_from_slice($crate::sha2::Digest::finalize(self.0).as_slice());
                $crate::Hash::from_bytes(out)
            }
        }
    $crate::impl_hasher!{ struct $name }
    )*};
}

#[macro_export]
macro_rules! blake2b_hasher {
    ($(struct $name:ident => $domain_sep:literal),+ $(,)? ) => {$(
        #[derive(Clone)]
        pub struct $name($crate::blake2b_simd::State);

        impl $name {
            #[inline(always)]
            pub fn new() -> Self {
                Self(
                    $crate::blake2b_simd::Params::new()
                        .hash_length(32)
                        .key($domain_sep)
                        .to_state(),
                )
            }

            pub fn write<A: AsRef<[u8]>>(&mut self, data: A) {
                self.0.update(data.as_ref());
            }

            #[inline(always)]
            pub fn finalize(self) -> $crate::Hash {
                let mut out = [0u8; 32];
                out.copy_from_slice(self.0.finalize().as_bytes());
                $crate::Hash::from_bytes(out)
            }
        }
    $crate::impl_hasher!{ struct $name }
    )*};
}

#[macro_export]
macro_rules! blake3_hasher {
    ($(struct $name:ident => $domain_sep:literal),+ $(,)? ) => {$(
        #[derive(Clone)]
        pub struct $name($crate::blake3::Hasher);

        impl $name {
            #[inline(always)]
            pub fn new() -> Self {
                const KEY: [u8; $crate::blake3::KEY_LEN] = {
                    let mut key = [0u8; $crate::blake3::KEY_LEN];
                    let mut i = 0usize;
                    while i < $domain_sep.len() {
                        key[i] = $domain_sep[i];
                        i += 1;
                    }
                    key
                };

                Self($crate::blake3::Hasher::new_keyed(&KEY))
            }

            pub fn write<A: AsRef<[u8]>>(&mut self, data: A) {
                self.0.update(data.as_ref());
            }

            #[inline(always)]
            pub fn finalize(self) -> $crate::Hash {
                let mut out = [0u8; 32];
                out.copy_from_slice(self.0.finalize().as_bytes());
                $crate::Hash::from_bytes(out)
            }
        }
    $crate::impl_hasher!{ struct $name }
    )*};
}

#[macro_export]
macro_rules! impl_hasher {
    (struct $name:ident) => {
        impl $crate::HasherBase for $name {
            #[inline(always)]
            fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
                self.write(data);
                self
            }
        }
        impl $crate::Hasher for $name {
            #[inline(always)]
            fn finalize(self) -> $crate::Hash {
                // Call the method
                $name::finalize(self)
            }
            #[inline(always)]
            fn reset(&mut self) {
                *self = Self::new();
            }
        }
        impl Default for $name {
            #[inline(always)]
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

use {blake2b_hasher, blake3_hasher, sha256_hasher};

#[cfg(test)]
mod tests {
    use super::*;
    use std::println;
    use std::string::ToString;

    fn run_test_vector<H: Hasher>(input_data: &[&[u8]], hasher_new: impl FnOnce() -> H, expected: &[&str]) {
        let mut hasher = hasher_new();
        // We do not reset the hasher each time on purpose, this also tests incremental hashing.
        for (data, expected) in input_data.iter().zip(expected) {
            println!("data: {data:?}");
            let hash = hasher.update(data).clone().finalize();
            assert_eq!(hash.to_string(), *expected, "Type: {}", std::any::type_name::<H>());
        }
    }

    #[test]
    fn test_vectors() {
        let input_data = [
            &[],
            &[1][..],
            &[
                5, 199, 126, 44, 71, 32, 82, 139, 122, 217, 43, 48, 52, 112, 40, 209, 180, 83, 139, 231, 72, 48, 136, 48, 168, 226,
                133, 7, 60, 4, 160, 205,
            ][..],
            &[42; 64],
            &[0; 8][..],
        ];

        run_test_vector(
            &input_data,
            TransactionHash::new,
            &[
                "50272a9e37c728026f93d0eda6ab4467f627338b879076483c88d291193cb3bf",
                "f9bf7e04c712621a0f4bb75d763f9ef5f73af6c438fd15b80744393bc96398ad",
                "8e791f3edcc92b71b8de2778efbc4666ee5bd146acbe8723a55bca26b022b0e0",
                "a6dab1a3088548c62d13a082fa28e870fdbbe51adcd8c364e2ea37e473c04d81",
                "3b79b78b967233843ad30f707b165eb3d6a91af8338076be8755c46a963c3d1d",
            ],
        );
        run_test_vector(
            &input_data,
            TransactionID::new,
            &[
                "e5f65efda0894d2b0590c2e9e46e9acc03032f505a1522f5e8c78c5ec70b1d9c",
                "aea52cf5e5a13da13a52dd69abd636eb1b0f86e58bc1dda6b17886b94593415a",
                "a50a2f87bdce075740189e9e23907ae22b5addbd875ccb70c116811b1fa5fb18",
                "0db7a485f7013a346a8f7f5caf73d52ca3c3b5ee101ad8753adedd4235b7236b",
                "2afc9c855854b0a6e94a722c3451d0cdfc8c11748b78ef65b9786f87b48d0d07",
            ],
        );

        run_test_vector(
            &input_data,
            TransactionSigningHash::new,
            &[
                "34c75037ad62740d4b3228f88f844f7901c07bfacd55a045be518eabc15e52ce",
                "8523b0471bcbea04575ccaa635eef9f9114f2890bda54367e5ff8caa3878bf82",
                "a51c49d9eb3d13f9de16e1aa8d1ff17668d55633ce00f36a643ac714b0fb137f",
                "487f199ef74c3e893e85bd37770e6334575a2d4d113b2e10474593c49807de93",
                "6392adc33a8e24e9a0a0c4c5f07f9c1cc958ad40c16d7a9a276e374cebb4e32b",
            ],
        );
        run_test_vector(
            &input_data,
            TransactionSigningHashECDSA::new,
            &[
                "b31ad1fbbe41b0e2a90e07c84708b38ba581f0c0e9185416913a04fb6d342027",
                "c43e1f75ea9df6379b56a95074c2b6289ed8c5a01fff2d49d9d44ad5575c164b",
                "49085f99fa0084b5436663f757a5916b1e4290c3321707fb76921ed4e47844ec",
                "3f887e866428de813c1d0463b14eef3ca1363c8187e917dda1eee0ec5996490b",
                "56de89a8c75f0fee2de61b11ab05d0d42e29ed50879467cf128dd80800a52ada",
            ],
        );

        run_test_vector(
            &input_data,
            BlockHash::new,
            &[
                "a80b6aa20f20b15ebabe2b1949527f78a257594a732e774de637d85e6973a768",
                "5643023add641f9421187b8c9aa3c6c73227d5ec34131c61a08d35b43e7e4b65",
                "4dc3bf72045431e46f8839a7d390898f27c887fddd8637149bfb70f732f04334",
                "15d7648e69023dca65c949a61ea166192049f449c604523494813873b19918a7",
                "3ac41af8385ea5d902ce6d47f509b7accc9c631f1d57a719d777874467f6d877",
            ],
        );

        run_test_vector(
            &input_data,
            MerkleBranchHash::new,
            &[
                "4de3617db456d01248173f17ec58196e92fbd994b636476db4b875ed2ec84054",
                "5737cd8b6fca5a30c19a491323a14e6b7021641cb3f8875f10c7a2eafd3cf43f",
                "a49eeda61cc75e0a8e5915829752fe0ad97620d6d32de7c9883595b0810ca33e",
                "28f33681dcff1313674e07dacc2d74c3089f6d8cea7a4f8792a71fd870988ee5",
                "2d53a43a42020a5091c125230bcd8a4cf0eeb188333e68325d4bce58a1c75ca3",
            ],
        );
    }

    #[test]
    fn test_blake3_vectors() {
        let input_data = [
            &[][..],
            &[1][..],
            &[
                5, 199, 126, 44, 71, 32, 82, 139, 122, 217, 43, 48, 52, 112, 40, 209, 180, 83, 139, 231, 72, 48, 136, 48, 168, 226,
                133, 7, 60, 4, 160, 205,
            ][..],
            &[42; 64],
            &[0; 8][..],
        ];

        run_test_vector(
            &input_data,
            SeqCommitLaneKey::new,
            &[
                "ffb48ccf5a7d24d449a8da0dd09a73b1e0a20ee10a7331c21930042e58d2aa24",
                "4daeb9c121484df93175020a60c04463b3c8b6ccfa4a06b7a8e6aff9ace7cfa4",
                "41a1aa1d34375a2ef257571a8ca52c73957483811cb4852a3b815fd72b3d773c",
                "a240e37f30a735d5791e589f4ca193e3ec801e24565f2f797faec33f88c10f9e",
                "a2df04afbd137a8ee9ed44a001af5dcc98cfffd14100163ebd2f899c542127a0",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitLaneTip::new,
            &[
                "255d4f981ed1e613ec87d46750254527c3044e42ce6b639dfa4190ab3965d777",
                "274025031a8f9b81346e0f1e0cd215929736c89bcf3b3589b744b196663dbf02",
                "72790c6e65fda13ddf64d48c062ad0f18d218f8a8c83c9426d13911ba60da4e6",
                "3359cef7e345290fa4f5270849ecdf154efcb06b189b7fbcf6889691cda5d3eb",
                "932c73359760c7f04ef38be45dd863b7a31372756c7f34ffec263edbc6b75362",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitActivityLeaf::new,
            &[
                "925e8cc9643d7b2ae93e38f7a0022f1f0c694eeb6608d991389f77be5f3dab3e",
                "de3a34c353bcd4dc1be835c5412d6fb3bcb55801750ce68f07ccd25d5d6446ee",
                "65abaa78c27d5400a4d28df2d6d6219939f4b4973012f651086fbef6f4159346",
                "5d3121321c6baaaec650f3746a1aea65b3310d1d544e8dc6a2d93ca6b0baf3c3",
                "dab38f396da66f90cedd52e3cf7ccf6846756cb1693465ef93791c657f8fc868",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitMergesetContext::new,
            &[
                "cd2b97a22304873dfa189e15c0a13636ea60570c11f763c6c1c6efc49d469489",
                "6cf9af3a32964ffea8b4eccd2cd2002d5b92cdb5ac11fea6ec1dfe229ddf652c",
                "55fa8db49590d0ef4f3d36e9c0b3b41e6791eaf8eda968f3499e66d952766e6e",
                "394d104cdb22f9429374dd15b6cb2e601876e6b97985803da9837a15d43aab42",
                "c8c5d211df896199dc349aeadc6da0fe3c3375f3539d1936268f9a6c64b6ed39",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitMinerPayload::new,
            &[
                "01099597e7a6d74e967ad54e6b64d721ae2a8a9c485794e96882034ec7e5e41d",
                "d479ea80ac9b84c5eb8408209ed706653e724b944976023e8aee10ac83360447",
                "593087cefc1b09fdc48620e39a86b347bc276ed86415b5fd4fe49a51004ffb9a",
                "0a92ac2be6f22125397c285b6f9a7631bce58498021fd1af8fa53f95f3ab86cd",
                "1d49cca0e56f64fa38daed45d84a84bac319633a20c66896878c166c2c82b20e",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitMinerPayloadLeaf::new,
            &[
                "1f53866c878ac122fe24c45ae3dd7931d9d045559f53ee64512aabf9dc1a0892",
                "4f0987631ad1a5a7b47c44309fc9690a5a1bab2351807d1282c6dabe1b06b235",
                "5dd4ed4f4a7fbc7054ec04df67f2acc8db259ca045614cb5390f6dc1b5a8275f",
                "fd4f1c71ecdff8ac4448a4ebccc540e6f30705c214ea62944bd41498e0409ed7",
                "b870bed32130d2e8a9697b4f205d9c9ddc26d4adcb5cf9df666acea8f071bb44",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitActiveLeaf::new,
            &[
                "9bccebe22e721372272c9d8520d2b3654bb93db82f41d77d460b682d97de1120",
                "3fde1c896d32bf8d24cff1ed22df7f5a76c10b45b37b5f836cdbbe3a9fe28a58",
                "a603c38c61e68d8fa7020cd1d4d6b07658985863192d589d0e6cffaa706e67cd",
                "35a96a743bc1521c7f5cc15968a407a82270c7b888fa0e4389d95afdbf0b0637",
                "4fd0302a50d9b4394a8db8bfb7e92d29f5731c89bb7e0523ebafed0098af76c2",
            ],
        );
        run_test_vector(
            &input_data,
            SeqCommitActiveNode::new,
            &[
                "6e1c8cc1d31bca65366d150b2d6ce3fff9436f5fb9549bb5f4e656c3e79bb385",
                "833ff5ffa0262a58ff7a062d1d8ad5f943b8433648b6699a7db4bfb144490c3b",
                "e4ba2f5b76d7385deeb8ca384c83ba47457bcdec14508f73ffb4c9a359c8c5b7",
                "ece4a35bea3f577e961f8a3cd6f6e140013166cd045cbc12a60e6029713a286b",
                "d66a35eca6ea9be29cbf16fa3e1da9d6550f62ae6db9d54f00a21f8315723594",
            ],
        );
    }
}
