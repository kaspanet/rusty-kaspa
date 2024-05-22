//! BIP39 mnemonic phrases

use super::{
    bits::{BitWriter, IterExt},
    language::Language,
};
use crate::Result;
use crate::{Error, KEY_SIZE};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_utils::hex::*;
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;
use zeroize::{Zeroize, Zeroizing};
use {super::seed::Seed, hmac::Hmac, sha2::Sha512};

/// Number of PBKDF2 rounds to perform when deriving the seed
const PBKDF2_ROUNDS: u32 = 2048;

/// Source entropy for a BIP39 mnemonic phrase
pub type Entropy32 = [u8; KEY_SIZE];
pub type Entropy16 = [u8; 16];

/// Word count for a BIP39 mnemonic phrase. Identifies mnemonic as 12 or 24 word variants.
#[derive(Default, Clone, Copy, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WordCount {
    #[default]
    Words12,
    Words24,
}

impl TryFrom<usize> for WordCount {
    type Error = Error;
    fn try_from(word_count: usize) -> Result<Self> {
        match word_count {
            12 => Ok(WordCount::Words12),
            24 => Ok(WordCount::Words24),
            _ => Err(Error::WordCount(word_count)),
        }
    }
}

/// BIP39 mnemonic phrases: sequences of words representing cryptographic keys.
/// @category Wallet SDK
#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct Mnemonic {
    /// Language
    language: Language,

    /// Source entropy for this phrase
    entropy: Vec<u8>,

    /// Mnemonic phrase
    phrase: String,
}

#[wasm_bindgen]
impl Mnemonic {
    #[wasm_bindgen(constructor)]
    pub fn constructor(phrase: String, language: Option<Language>) -> Result<Mnemonic> {
        Mnemonic::new(phrase, language.unwrap_or(Language::English))
    }

    /// Validate mnemonic phrase. Returns `true` if the phrase is valid, `false` otherwise.
    pub fn validate(phrase: &str, language: Option<Language>) -> bool {
        Mnemonic::new(phrase, language.unwrap_or(Language::English)).is_ok()
    }

    #[wasm_bindgen(getter, js_name = entropy)]
    pub fn get_entropy(&self) -> String {
        self.entropy.to_hex()
    }

    #[wasm_bindgen(setter, js_name = entropy)]
    pub fn set_entropy(&mut self, entropy: String) {
        let vec = Vec::<u8>::from_hex(&entropy).unwrap_or_else(|err| panic!("invalid entropy `{entropy}`: {err}"));
        let len = vec.len();
        if len != 16 && len != 32 {
            panic!("Invalid entropy: `{entropy}`")
        }
        self.entropy = vec;
    }

    #[wasm_bindgen(js_name = random)]
    pub fn create_random_js(word_count: JsValue) -> Result<Mnemonic> {
        let word_count = word_count.as_f64().unwrap_or(24.0) as usize;
        Mnemonic::random(word_count.try_into()?, Default::default())
    }

    #[wasm_bindgen(getter, js_name = phrase)]
    pub fn phrase_string(&self) -> String {
        self.phrase.clone()
    }

    #[wasm_bindgen(setter, js_name = phrase)]
    pub fn set_phrase(&mut self, phrase: &str) {
        self.phrase = phrase.to_string();
    }

    #[wasm_bindgen(js_name = toSeed)]
    pub fn create_seed(&self, password: Option<String>) -> String {
        let password = password.unwrap_or_default();
        self.to_seed(password.as_str()).as_bytes().to_vec().to_hex()
    }
}

impl Mnemonic {
    pub fn random(word_count: WordCount, language: Language) -> Result<Mnemonic> {
        Mnemonic::random_impl(word_count, rand::thread_rng(), language)
    }

    /// Create a random BIP39 mnemonic phrase.
    pub fn random_impl(word_count: WordCount, mut rng: impl RngCore + CryptoRng, language: Language) -> Result<Self> {
        match word_count {
            WordCount::Words24 => {
                let mut entropy = Entropy32::default();
                rng.fill_bytes(&mut entropy);
                Self::from_entropy(entropy.to_vec(), language)
            }
            WordCount::Words12 => {
                let mut entropy = Entropy16::default();
                rng.fill_bytes(&mut entropy);
                Self::from_entropy(entropy.to_vec(), language)
            }
        }
    }

    /// Create a new BIP39 mnemonic phrase from the given entropy
    pub fn from_entropy(entropy: Vec<u8>, language: Language) -> Result<Self> {
        if entropy.len() != 16 && entropy.len() != 32 {
            return Err(Error::String("Entropy length should be 16 or 32.".to_string()));
        }

        let wordlist = language.wordlist();
        let entropy = Zeroizing::new(entropy);
        let checksum_byte = Self::build_checksum(&entropy)?;

        // First, create a byte iterator for the given entropy and the first byte of the
        // hash of the entropy that will serve as the checksum (up to 8 bits for biggest
        // entropy source).
        //
        // Then we transform that into a bits iterator that returns 11 bits at a
        // time (as u16), which we can map to the words on the `wordlist`.
        //
        // Given the entropy is of correct size, this ought to give us the correct word
        // count.
        let phrase = entropy.iter().chain(Some(&checksum_byte)).bits().map(|bits| wordlist.get_word(bits)).join(" ");

        Ok(Self { language, entropy: entropy.to_vec(), phrase })
    }

    /// Create a new BIP39 mnemonic phrase from the given string.
    ///
    /// The phrase supplied will be checked for word length and validated
    /// according to the checksum specified in BIP0039.
    ///
    /// To use the default language, English, (the only one supported by this
    /// library and also the only one standardized for BIP39) you can supply
    /// `Default::default()` as the language.
    pub fn new<S>(phrase: S, language: Language) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let phrase = phrase.as_ref();
        let wordmap = language.wordmap();

        // Preallocate enough space for the longest possible word list
        let mut bits = BitWriter::with_capacity(264);

        for word in phrase.split(' ') {
            bits.push(wordmap.get_bits(word).ok_or(Error::Bip39)?);
        }

        let mut entropy = Zeroizing::new(bits.into_bytes());

        let key_size = match entropy.len() {
            17 => 16,
            33 => 32,
            _ => {
                return Err(Error::Bip39);
            }
        };

        let actual_checksum = entropy[key_size];

        // Truncate to get rid of the byte containing the checksum
        entropy.truncate(key_size);

        let expected_checksum = Self::build_checksum(&entropy)?;

        if actual_checksum != expected_checksum {
            return Err(Error::String(format!("BIP39: actual checksum({actual_checksum}) != expected checksum({expected_checksum})")));
        }

        Self::from_entropy(entropy.to_vec(), language)
    }

    fn build_checksum(entropy: &Zeroizing<Vec<u8>>) -> Result<u8> {
        let binding = Sha256::digest(entropy);
        let bytes = binding.as_slice();
        //println!("len: {}, bytes: {:?}", entropy.len(), bytes);
        match entropy.len() {
            16 => {
                let checksum = bytes[0] & 0b11110000;
                Ok(checksum)
            }
            32 => Ok(bytes[0]),
            // 64=>{}
            _ => Err(Error::Bip39),
        }
    }

    /// Get source entropy for this phrase.
    pub fn entropy(&self) -> &Vec<u8> {
        &self.entropy
    }

    /// Get the mnemonic phrase as a string reference.
    pub fn phrase(&self) -> &str {
        &self.phrase
    }

    /// Language this phrase's wordlist is for
    pub fn language(&self) -> Language {
        self.language
    }

    /// Convert this mnemonic phrase into the BIP39 seed value.
    //#[cfg(feature = "bip39")]
    //#[cfg_attr(docsrs, doc(cfg(feature = "bip39")))]
    pub fn to_seed(&self, password: &str) -> Seed {
        let salt = Zeroizing::new(format!("mnemonic{password}"));
        let mut seed = [0u8; Seed::SIZE];
        pbkdf2::pbkdf2::<Hmac<Sha512>>(self.phrase.as_bytes(), salt.as_bytes(), PBKDF2_ROUNDS, &mut seed).unwrap();
        Seed(seed)
    }
}

impl Drop for Mnemonic {
    fn drop(&mut self) {
        self.phrase.zeroize();
        self.entropy.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::Mnemonic;
    use crate::ExtendedPrivateKey;
    use crate::Language;
    use crate::Prefix;
    use crate::SecretKey;

    #[test]
    pub fn tests() {
        let data = [
            [
                "caution guide valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light nut invite fiction visa hamster coyote",
                "xprv9s21ZrQH143K4VcEtb888srakzAVaKWE9L3Pyy6AxEhWLtJu5FK18mDHE1ar7LHT99KrrqcVQxRMSqnXj65vsuWDGUxeT3AVKVB7YW8FDoD"
            ],
            [
                "catalog girl about bench aunt kingdom moment example height mesh sentence usual once era stand bachelor wash pulse heavy wool drift few uncover security",
                "xprv9s21ZrQH143K2wVjriV7iBheUcs5So6hqi9cZVbikCJu8CY2YkjGe9ciL9P1pwgqJZjDqkTcxkw5ZmykHd8G9KEr8UE2FTM45NdB3so7su5"
            ],
            [
                "sign alarm peace aisle panther century wink action mad series absurd planet trigger casino radio neck vehicle resist segment dial aim slim yard cousin",
                "xprv9s21ZrQH143K2KaT25wF5RaJmwyoKGyvJWtha4vp9dHSeft2U45ugLp3kQPzjw8bnMRcjGi6v4EHm5AMY2AmXvkHnscpp41oTzgPS9kaUsy"
            ],
            [
                "annual school will jazz response giant decide display beef slush together still water victory south eyebrow adult nasty minor twist empty caught smooth only",
                "xprv9s21ZrQH143K327fsEDJyFE3KXMsbohV237ET6FXnMfixZQJ5Q1myJwos2aGeBfbwmtpxPoAewr2XpKgc3rUAA9UbmYd3aFief6Q3qyu5gT"
            ],
            [
                "advance defy bridge record time fringe heart useful delay grab fresh axis blouse hidden approve labor come wise satisfy silly honey spice bulb maximum",
                "xprv9s21ZrQH143K2ey6aizH6RGVDQgzWu3VfgCaXQds9LXpsyT8mdDeBGBGdWCdLVjxhCBvfR98WSffrDiGYc6RwkgbXneyZudBFv41tRT5yZA"
            ],
            [
                "social anchor educate fold ancient wheel advice praise file fix attitude ivory",
                "xprv9s21ZrQH143K3ZsozYTEYBEJ4wL4MRPMrvXcNNmqNfbEuWKqMgRDD2psd7HrE5yWEd2UFK2TtdEJnfDFNYibjKzMGz7uYdze5vbjGRZHKmU"
            ],
            [
                "mosquito also bubble sugar brother concert can comic sick hip abstract hard",
                "xprv9s21ZrQH143K3dd8qYhu8xnCRA2utL1rPUfwmrkPLkvy3RReQPQQyxdiwP7oJ5tbKK3YNZyZNdahCyLowh4kajU5WLtBg8CC1fGXJBhGKMR"
            ],
            [
                "mother dress law arena peasant camera forum wisdom mutual inform silk regret",
                "xprv9s21ZrQH143K3QTdXMBavciDtwpgdgjKwu9WEJsc1rBdwjq11PsckuaNwhMGr8gDTUuKZaV8dDemXZuprfcqNhLDT3TJ16Kpq1DAFZ35WSE"
            ],
            [
                "client response wonder quote fork awake toddler flower lawn mean poem traffic",
                "xprv9s21ZrQH143K2Zx4T5nypc1daCXrZrq6mU79mJSVJT7mXLiCkHoStb3imvDJP5tU8YTAZQawb7imhBG5D12jXzmggxFY2sXntq2nfAgmjYc"
            ],
            [
                "topple outdoor twelve earth dragon misery senior miss square unhappy hand appear",
                "xprv9s21ZrQH143K2YStJyGeTyoWRBu2N1wkamjidQSdxrVeDziGfvwkmP67L2xf6weijVapZxwi64pW8ywHDvCaBQA8PyrRHqkjuuPY9aapypz"
            ],
            [
                // KPRV (kaspa mainnet xprv)
                "cruise village slam canyon monster scrub myself farm add riot large board sentence outer nice coast raven bird scheme undo december blanket trim hero",
                "kprv5y2qurMHCsXYr8yytxy6ZwYWLtFbdtWWavDL6bPfz2fNLvnZymmNfE6KpQqNHHjb7mAWYCtuUkZPbkgUR19LSKS9VasqRR852L5GMVY8wf9"
            ],
            [
                // KTRV (kaspa testnet xprv)
                "short diagram life tip retreat nothing dynamic absent lamp carry mansion keen truck cram crash science liberty emotion live pepper orphan quiz wide prison",
                "ktrv5himbbCxArFU23gGTxVHNKahNXXSETHjNWgwc5qm85nKS1p55FEb8DUdTd2CPvQvBUKYFRSjjXb5nagr7wXUE4eSaFSxof8cUd6Sm66NRjA"
            ]
        ];

        for [seed_words, xprv_str] in data {
            let mnemonic = match Mnemonic::new(seed_words, Language::English) {
                Ok(v) => v,
                Err(err) => {
                    println!("Mnemonic::new:err {err:?}, seed_words: {seed_words}");
                    return;
                }
            };

            let seed = mnemonic.to_seed("");
            let xprv = ExtendedPrivateKey::<SecretKey>::new(seed).unwrap();
            let prefix = if xprv_str.starts_with("kp") {
                Prefix::KPRV
            } else if xprv_str.starts_with("kt") {
                Prefix::KTRV
            } else {
                Prefix::XPRV
            };
            assert_eq!(&xprv.to_string(prefix).to_string(), xprv_str, "xprv is not valid");
        }
    }
}
