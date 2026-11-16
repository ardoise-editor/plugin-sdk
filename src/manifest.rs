//! `plugin.json`: what a plugin is, how to start it and what it adds to
//! the editor. It sits at the root of the plugin's folder.
//!
//! ```json
//! {
//!   "id": "acme.hello",
//!   "name": "Hello",
//!   "version": "0.1.0",
//!   "api": 1,
//!   "description": { "en": "Says hello", "fr": "Dit bonjour" },
//!   "command": ["bin/hello"],
//!   "events": ["file_saved"],
//!   "contributes": {
//!     "commands": [{ "id": "hello.greet", "title": "Say hello" }],
//!     "panels": [{ "id": "hello", "title": "Hello", "icon": "H" }]
//!   }
//! }
//! ```

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::protocol::API_VERSION;

/// The name of the file.
pub const FILE: &str = "plugin.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Unique: lowercase words separated by dots, the first being the
    /// author (`acme.hello`). `simpleedit.*` is kept for the official ones.
    pub id: String,
    pub name: Localized,
    /// `MAJOR.MINOR.PATCH`.
    pub version: String,
    /// The protocol version the plugin speaks ([`API_VERSION`]).
    pub api: u32,
    #[serde(default)]
    pub description: Localized,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// `owner/name` of its GitHub repository, where its releases are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// The program to start and its arguments. A first word with a `/` is a
    /// file of the plugin's folder (`bin/hello`); otherwise it is looked for
    /// on the `PATH` (`python3`, `node`).
    pub command: Vec<String>,
    /// What the plugin wants to hear about. A plugin that asks for any is
    /// started with the editor; the others when first needed (a command, a
    /// panel).
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub contributes: Contributes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// Only to be started with the editor.
    Startup,
    FileOpened,
    FileSaved,
    FileClosed,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    /// Listed in the editor's search for actions (Ctrl/Cmd+Shift+A).
    #[serde(default)]
    pub commands: Vec<Command>,
    #[serde(default)]
    pub panels: Vec<Panel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub id: String,
    pub title: Localized,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Panel {
    pub id: String,
    pub title: Localized,
    /// One character for its button: a Nerd Fonts glyph or a symbol. The
    /// first letter of the title if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub location: Location,
}

/// Where a panel opens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    /// On the left, in place of the file list; its button is at the top of
    /// the activity bar.
    #[default]
    Sidebar,
    /// Under the editor, like the terminal; its button is at the bottom of
    /// the activity bar.
    Bottom,
}

/// A text in one language, or in several: `"Hello"` or
/// `{"en": "Hello", "fr": "Bonjour"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Localized {
    Text(String),
    Translated(BTreeMap<String, String>),
}

impl Default for Localized {
    fn default() -> Self {
        Localized::Text(String::new())
    }
}

impl Localized {
    /// The text in `locale`, else in English, else the first there is.
    pub fn get(&self, locale: &str) -> &str {
        match self {
            Localized::Text(text) => text,
            Localized::Translated(map) => map
                .get(locale)
                .or_else(|| locale.split(['-', '_']).next().and_then(|l| map.get(l)))
                .or_else(|| map.get("en"))
                .or_else(|| map.values().next())
                .map_or("", String::as_str),
        }
    }

    fn is_blank(&self) -> bool {
        match self {
            Localized::Text(text) => text.trim().is_empty(),
            Localized::Translated(map) => map.values().all(|t| t.trim().is_empty()),
        }
    }
}

impl From<&str> for Localized {
    fn from(text: &str) -> Self {
        Localized::Text(text.to_string())
    }
}

/// Why a manifest was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalid {
    Json(String),
    Id,
    Name,
    Version,
    /// Written for a protocol this editor does not speak.
    Api(u32),
    Command,
    /// A command or panel without an id, with one used twice, or without a title.
    Contribution(String),
}

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Invalid::Json(e) => write!(f, "plugin.json: {e}"),
            Invalid::Id => f.write_str("plugin.json: \"id\" must be like \"author.name\""),
            Invalid::Name => f.write_str("plugin.json: \"name\" is empty"),
            Invalid::Version => f.write_str("plugin.json: \"version\" must be MAJOR.MINOR.PATCH"),
            Invalid::Api(v) => write!(
                f,
                "plugin.json: \"api\" {v} is not supported (this editor speaks {API_VERSION})"
            ),
            Invalid::Command => f.write_str(
                "plugin.json: \"command\" must name a program (a file of the plugin with no \"..\", or a name)",
            ),
            Invalid::Contribution(id) => write!(f, "plugin.json: contribution \"{id}\" is invalid"),
        }
    }
}

impl std::error::Error for Invalid {}

impl Manifest {
    /// Reads and checks a `plugin.json`.
    pub fn parse(json: &str) -> Result<Self, Invalid> {
        let manifest: Manifest =
            serde_json::from_str(json).map_err(|e| Invalid::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), Invalid> {
        if !valid_id(&self.id) {
            return Err(Invalid::Id);
        }
        if self.name.is_blank() {
            return Err(Invalid::Name);
        }
        if parse_version(&self.version).is_none() {
            return Err(Invalid::Version);
        }
        if self.api != API_VERSION {
            return Err(Invalid::Api(self.api));
        }
        let Some(program) = self.command.first() else {
            return Err(Invalid::Command);
        };
        let bad_part = program
            .split(['/', '\\'])
            .any(|part| part == ".." || part == ".");
        if program.trim().is_empty()
            || program.starts_with('/')
            || program.contains('\\')
            || bad_part
        {
            return Err(Invalid::Command);
        }
        let mut seen = HashSet::new();
        let commands = self.contributes.commands.iter().map(|c| (&c.id, &c.title));
        let panels = self.contributes.panels.iter().map(|p| (&p.id, &p.title));
        for (id, title) in commands.chain(panels) {
            if !valid_name(id) || title.is_blank() || !seen.insert(id.as_str()) {
                return Err(Invalid::Contribution(id.clone()));
            }
        }
        for panel in &self.contributes.panels {
            if panel.icon.as_ref().is_some_and(|i| i.chars().count() > 2) {
                return Err(Invalid::Contribution(panel.id.clone()));
            }
        }
        Ok(())
    }

    /// Whether the plugin is started with the editor.
    pub fn starts_with_editor(&self) -> bool {
        !self.events.is_empty()
    }

    /// The program is a file of the plugin (rather than one on the `PATH`).
    pub fn runs_own_file(&self) -> bool {
        self.command.first().is_some_and(|p| p.contains('/'))
    }
}

/// `author.name`: two or more words of `a-z`, `0-9` and `-`, dot-separated.
pub fn valid_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    id.len() <= 64
        && parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('-')
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// The id of a command or a panel: letters, digits, `.`, `-`, `_`.
fn valid_name(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Strict `MAJOR.MINOR.PATCH` (a leading `v` is allowed, as in tags).
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.');
    let mut next = || {
        let p = parts.next()?;
        (!p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse().ok())
            .flatten()
    };
    let v = (next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = r#"{
        "id": "acme.hello",
        "name": {"en": "Hello", "fr": "Bonjour"},
        "version": "0.1.0",
        "api": 1,
        "command": ["bin/hello", "--quiet"],
        "contributes": {
            "commands": [{"id": "hello.greet", "title": "Say hello"}],
            "panels": [{"id": "hello", "title": "Hello", "icon": "H", "location": "bottom"}]
        }
    }"#;

    #[test]
    fn a_good_manifest_is_read_whole() {
        let m = Manifest::parse(HELLO).unwrap();
        assert_eq!(m.name.get("fr"), "Bonjour");
        assert_eq!(m.name.get("de"), "Hello");
        assert_eq!(m.name.get("fr-CA"), "Bonjour");
        assert_eq!(m.contributes.panels[0].location, Location::Bottom);
        assert!(m.runs_own_file());
        assert!(!m.starts_with_editor());
    }

    #[test]
    fn bad_manifests_say_why() {
        let with = |from: &str, to: &str| Manifest::parse(&HELLO.replace(from, to));
        assert_eq!(with("acme.hello", "Hello"), Err(Invalid::Id));
        assert_eq!(with("acme.hello", "acme..x"), Err(Invalid::Id));
        assert_eq!(with("0.1.0", "1.0"), Err(Invalid::Version));
        assert_eq!(with("\"api\": 1", "\"api\": 9"), Err(Invalid::Api(9)));
        assert_eq!(with("bin/hello", "../hello"), Err(Invalid::Command));
        assert_eq!(with("bin/hello", "/usr/bin/hello"), Err(Invalid::Command));
        assert_eq!(with("bin/hello", "bin/./x"), Err(Invalid::Command));
        assert_eq!(
            with("\"id\": \"hello\"", "\"id\": \"hello.greet\""),
            Err(Invalid::Contribution("hello.greet".into()))
        );
        assert!(matches!(
            with("\"api\"", "\"other\": 1, \"api\""),
            Err(Invalid::Json(_))
        ));
        assert!(with("bin/hello", "python3").is_ok());
    }

    #[test]
    fn versions_are_strict() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-beta"), None);
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }
}
