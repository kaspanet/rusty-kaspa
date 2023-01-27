//! BIP39 mnemonic phrases

use super::{
    bits::{BitWriter, IterExt},
    language::Language,
};
use crate::{Error, KEY_SIZE};
//use alloc::{format, string::String};
use rand_core::{CryptoRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

//#[cfg(feature = "bip39")]
use {super::seed::Seed, hmac::Hmac, sha2::Sha512};

/// Number of PBKDF2 rounds to perform when deriving the seed
//#[cfg(feature = "bip39")]
const PBKDF2_ROUNDS: u32 = 2048;

/// Source entropy for a BIP39 mnemonic phrase
pub type Entropy = [u8; KEY_SIZE];

/// BIP39 mnemonic phrases: sequences of words representing cryptographic keys.
#[derive(Clone)]
pub struct Mnemonic {
    /// Language
    language: Language,

    /// Source entropy for this phrase
    entropy: Entropy,

    /// Mnemonic phrase
    phrase: String,
}

impl Mnemonic {
    /// Create a random BIP39 mnemonic phrase.
    pub fn random(mut rng: impl RngCore + CryptoRng, language: Language) -> Self {
        let mut entropy = Entropy::default();
        rng.fill_bytes(&mut entropy);
        Self::from_entropy(entropy, language)
    }

    /// Create a new BIP39 mnemonic phrase from the given entropy
    pub fn from_entropy(entropy: Entropy, language: Language) -> Self {
        let wordlist = language.wordlist();
        let checksum_byte = Sha256::digest(entropy.as_ref()).as_slice()[0];

        // First, create a byte iterator for the given entropy and the first byte of the
        // hash of the entropy that will serve as the checksum (up to 8 bits for biggest
        // entropy source).
        //
        // Then we transform that into a bits iterator that returns 11 bits at a
        // time (as u16), which we can map to the words on the `wordlist`.
        //
        // Given the entropy is of correct size, this ought to give us the correct word
        // count.
        let phrase = entropy
            .iter()
            .chain(Some(&checksum_byte))
            .bits()
            .map(|bits| wordlist.get_word(bits))
            .join(" ");

        Self {
            language,
            entropy,
            phrase,
        }
    }

    /// Create a new BIP39 mnemonic phrase from the given string.
    ///
    /// The phrase supplied will be checked for word length and validated
    /// according to the checksum specified in BIP0039.
    ///
    /// To use the default language, English, (the only one supported by this
    /// library and also the only one standardized for BIP39) you can supply
    /// `Default::default()` as the language.
    pub fn new<S>(phrase: S, language: Language) -> Result<Self, Error>
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

        if entropy.len() != KEY_SIZE + 1 {
            return Err(Error::Bip39);
        }

        let actual_checksum = entropy[KEY_SIZE];

        // Truncate to get rid of the byte containing the checksum
        entropy.truncate(KEY_SIZE);

        let expected_checksum = Sha256::digest(&*entropy).as_slice()[0];

        if actual_checksum != expected_checksum {
            return Err(Error::Bip39);
        }

        Ok(Self::from_entropy(
            entropy.as_slice().try_into().map_err(|_| Error::Bip39)?,
            language,
        ))
    }

    /// Get source entropy for this phrase.
    pub fn entropy(&self) -> &Entropy {
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
        let salt = Zeroizing::new(format!("mnemonic{}", password));
        let mut seed = [0u8; Seed::SIZE];
        pbkdf2::pbkdf2::<Hmac<Sha512>>(
            self.phrase.as_bytes(),
            salt.as_bytes(),
            PBKDF2_ROUNDS,
            &mut seed,
        );
        Seed(seed)
    }
}

impl Drop for Mnemonic {
    fn drop(&mut self) {
        self.phrase.zeroize();
        self.entropy.zeroize();
    }
}
