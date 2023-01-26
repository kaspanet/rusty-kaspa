//! Wordlist support
//!
//! NOTE: This implementation is not constant time and may leak information
//! via timing side-channels!
//!
//! Adapted from the `bip39` crate

use super::bits::{Bits, Bits11};
use std::{collections::BTreeMap, vec::Vec};

/// Supported languages.
///
/// Presently only English is specified by the BIP39 standard
#[derive(Copy, Clone, Debug)]
pub enum Language {
    /// English is presently the only supported language
    English,
}

impl Language {
    /// Get the word list for this language
    pub(crate) fn wordlist(&self) -> &'static WordList {
        match *self {
            Language::English => &lazy::WORDLIST_ENGLISH,
        }
    }

    /// Get a wordmap that allows word -> index lookups in the word list
    pub(crate) fn wordmap(&self) -> &'static WordMap {
        match *self {
            Language::English => &lazy::WORDMAP_ENGLISH,
        }
    }
}

impl Default for Language {
    fn default() -> Language {
        Language::English
    }
}

pub(crate) struct WordMap {
    inner: BTreeMap<&'static str, Bits11>,
}

pub(crate) struct WordList {
    inner: Vec<&'static str>,
}

impl WordMap {
    pub fn get_bits(&self, word: &str) -> Option<Bits11> {
        self.inner.get(word).cloned()
    }
}

impl WordList {
    pub fn get_word(&self, bits: Bits11) -> &'static str {
        self.inner[bits.bits() as usize]
    }
}

// TODO(tarcieri): use `const fn` instead of `Lazy`
mod lazy {
    use super::{Bits11, WordList, WordMap};
    //use alloc::vec::Vec;
    use once_cell::sync::Lazy;

    /// lazy generation of the word list
    fn gen_wordlist(lang_words: &'static str) -> WordList {
        let inner: Vec<_> = lang_words.split_whitespace().collect();

        debug_assert!(inner.len() == 2048, "Invalid wordlist length");

        WordList { inner }
    }

    /// lazy generation of the word map
    fn gen_wordmap(wordlist: &WordList) -> WordMap {
        let inner = wordlist
            .inner
            .iter()
            .enumerate()
            .map(|(i, item)| (*item, Bits11::from(i as u16)))
            .collect();

        WordMap { inner }
    }

    pub(crate) static WORDLIST_ENGLISH: Lazy<WordList> =
        Lazy::new(|| gen_wordlist(include_str!("words/english.txt")));

    pub(crate) static WORDMAP_ENGLISH: Lazy<WordMap> = Lazy::new(|| gen_wordmap(&WORDLIST_ENGLISH));
}
