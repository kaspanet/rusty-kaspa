//! Wordlist support
//!
//! NOTE: This implementation is not constant time and may leak information
//! via timing side-channels!
//!
//! Adapted from the `bip39` crate

use super::bits::{Bits, Bits11};
use std::{collections::BTreeMap, vec::Vec};
use wasm_bindgen::prelude::*;

///
/// Languages supported by BIP39.
///
/// Presently only English is specified by the BIP39 standard.
///
/// @see {@link Mnemonic}
///
/// @category Wallet SDK
#[derive(Copy, Clone, Debug, Default)]
#[wasm_bindgen]
pub enum Language {
    /// English is presently the only supported language
    #[default]
    English,
}

impl Language {
    /// Get the word list for this language
    pub fn wordlist(&self) -> &'static WordList {
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

pub(crate) struct WordMap {
    inner: BTreeMap<&'static str, Bits11>,
}

pub struct WordList {
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

    pub fn iter(&self) -> WordListIterator<'_> {
        WordListIterator { wordlist: self, index: 0 }
    }
}

pub struct WordListIterator<'a> {
    wordlist: &'a WordList,
    index: usize,
}

impl Iterator for WordListIterator<'_> {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.wordlist.inner.len() {
            let word = self.wordlist.inner[self.index];
            self.index += 1;
            Some(word)
        } else {
            None
        }
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
        let inner = wordlist.inner.iter().enumerate().map(|(i, item)| (*item, Bits11::from(i as u16))).collect();

        WordMap { inner }
    }

    pub(crate) static WORDLIST_ENGLISH: Lazy<WordList> = Lazy::new(|| gen_wordlist(include_str!("words/english.txt")));

    pub(crate) static WORDMAP_ENGLISH: Lazy<WordMap> = Lazy::new(|| gen_wordmap(&WORDLIST_ENGLISH));
}
