//! Spelling, by a plugin that declares `contributes.speller`: the editor
//! underlines the words it says are misspelled, and shows its suggestions.

use serde::{Deserialize, Serialize};

/// A language the plugin has a dictionary for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    /// `fr`, `en_US`…
    pub code: String,
}

/// `spell/check`: the misspelled words of a text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckParams {
    pub text: String,
    /// The language of the file as the editor highlights it (`rust`,
    /// `markdown`, empty for plain text): in a program, only the comments
    /// and strings are prose.
    #[serde(default)]
    pub syntax: String,
    /// The language chosen by the user (empty: that of the interface).
    #[serde(default)]
    pub language: String,
    /// The language of the interface (`fr`).
    #[serde(default)]
    pub interface: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// The language the text was checked in (none: no dictionary for it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The misspelled words, as byte ranges `[start, end)` of the text.
    #[serde(default)]
    pub misspelled: Vec<(usize, usize)>,
}

/// `spell/suggest`: what could replace a word.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestParams {
    pub word: String,
    /// The language it was checked in (from [`CheckResult::language`]).
    pub language: String,
}

/// `spell/learn`: a word to know from now on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnParams {
    pub word: String,
    /// For ever (in the personal dictionary), or only until the editor
    /// closes.
    #[serde(default)]
    pub keep: bool,
}
