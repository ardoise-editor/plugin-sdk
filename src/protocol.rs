//! What the editor and a plugin say to each other.
//!
//! A plugin is a program the editor starts. They speak [JSON-RPC 2.0] over
//! the plugin's standard input and output, each message framed as in the
//! Language Server Protocol: a `Content-Length: <bytes>` header, an empty
//! line, then the JSON body. The standard error of the plugin goes to its
//! log file.
//!
//! [JSON-RPC 2.0]: https://www.jsonrpc.org/specification

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ui::Node;

/// The version of the protocol this crate speaks. A plugin declares the one
/// it was written for in its manifest (`"api"`); the editor starts only the
/// plugins whose version it knows.
pub const API_VERSION: u32 = 1;

/// The largest message either side accepts.
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;

/// The names of the methods.
pub mod methods {
    // ── From the editor to the plugin ──────────────────────────────────
    /// Request, [`InitializeParams`](super::InitializeParams): the first
    /// message. The plugin answers once it is ready.
    pub const INITIALIZE: &str = "initialize";
    /// Request, no parameters: the plugin should stop. The editor closes
    /// its input after the answer (or after 2 seconds).
    pub const SHUTDOWN: &str = "shutdown";
    /// Notification, [`CommandParams`](super::CommandParams): the user ran a
    /// command of the plugin.
    pub const COMMAND: &str = "command";
    /// Notification, [`PanelParams`](super::PanelParams): a panel of the
    /// plugin was opened; the plugin should send its content (`ui/render`).
    pub const PANEL_OPENED: &str = "panel/opened";
    /// Notification, [`PanelParams`](super::PanelParams).
    pub const PANEL_CLOSED: &str = "panel/closed";
    /// Notification, [`UiEvent`](super::UiEvent): the user pressed, typed or
    /// picked something in a panel.
    pub const UI_EVENT: &str = "ui/event";
    /// Notification, [`FileParams`](super::FileParams). Sent only to the
    /// plugins that asked for `file_opened` in their manifest.
    pub const FILE_OPENED: &str = "event/fileOpened";
    /// Notification, [`FileParams`](super::FileParams) (`file_saved`): a
    /// file was written by the editor, or changed on the disk while open.
    pub const FILE_SAVED: &str = "event/fileSaved";
    /// Notification, [`FileParams`](super::FileParams) (`file_closed`).
    pub const FILE_CLOSED: &str = "event/fileClosed";
    /// Notification, [`FileParams`](super::FileParams) with the text of the
    /// file in `previous` (`file_deleted`): it is about to go.
    pub const FILE_DELETED: &str = "event/fileDeleted";
    /// Notification, [`RenameParams`](super::RenameParams) (`file_renamed`).
    pub const FILE_RENAMED: &str = "event/fileRenamed";
    /// Notification, [`ActiveFileParams`](super::ActiveFileParams): the
    /// file in front changed (`active_file` in the manifest).
    pub const ACTIVE_FILE_CHANGED: &str = "event/activeFileChanged";
    /// Notification, [`PanelParams`](super::PanelParams) with the tab as
    /// `panel`: a tab of the plugin was closed.
    pub const TAB_CLOSED: &str = "tab/closed";
    /// Notification, [`PanelParams`](super::PanelParams): the user asked to
    /// save the tab (Ctrl/Cmd+S).
    pub const TAB_SAVE: &str = "tab/save";
    /// Notification, [`Settings`](super::Settings): the language or the
    /// theme of the editor changed.
    pub const SETTINGS_CHANGED: &str = "event/settingsChanged";
    /// Request, [`LintParams`](crate::lint::LintParams) →
    /// [`LintResult`](crate::lint::LintResult): the problems of a file.
    /// Only to the plugins with `contributes.linter`.
    pub const LINT_RUN: &str = "lint/run";
    /// Request, [`FixParams`](crate::lint::FixParams) →
    /// [`FixResult`](crate::lint::FixResult): the text with them fixed.
    pub const LINT_FIX: &str = "lint/fix";
    /// Request, no parameters → `[Language]`
    /// ([`spell::Language`](crate::spell::Language)): the dictionaries.
    /// Only to the plugins with `contributes.speller`.
    pub const SPELL_LANGUAGES: &str = "spell/languages";
    /// Request, [`CheckParams`](crate::spell::CheckParams) →
    /// [`CheckResult`](crate::spell::CheckResult).
    pub const SPELL_CHECK: &str = "spell/check";
    /// Request, [`SuggestParams`](crate::spell::SuggestParams) → `[String]`.
    pub const SPELL_SUGGEST: &str = "spell/suggest";
    /// Request, [`LearnParams`](crate::spell::LearnParams) → `null`.
    pub const SPELL_LEARN: &str = "spell/learn";

    // ── From the plugin to the editor ──────────────────────────────────
    /// Notification, [`RenderParams`](super::RenderParams): what a panel
    /// shows, whole.
    pub const RENDER: &str = "ui/render";
    /// Request, [`TabParams`](super::TabParams) → `null`: opens a tab of the
    /// plugin among the files (or brings it in front). It shows what the
    /// plugin renders for the panel named as the tab; after a restart, the
    /// editor sends `panel/opened` for it when it is first shown.
    pub const OPEN_TAB: &str = "editor/openTab";
    /// Notification, [`TabDirtyParams`](super::TabDirtyParams): the tab
    /// holds changes not kept (its name is marked, closing it asks first).
    pub const TAB_DIRTY: &str = "editor/tabDirty";
    /// Notification, [`PanelParams`](super::PanelParams): closes a tab.
    pub const CLOSE_TAB: &str = "editor/closeTab";
    /// Notification, [`PanelParams`](super::PanelParams): opens a panel of
    /// the plugin (as its button would), if it is not open.
    pub const SHOW_PANEL: &str = "ui/showPanel";
    /// Notification, [`NotifyParams`](super::NotifyParams): a message in the
    /// status bar.
    pub const NOTIFY: &str = "window/notify";
    /// Request, [`ConfirmParams`](super::ConfirmParams) → `bool`: asks the
    /// user a yes / no question.
    pub const CONFIRM: &str = "window/confirm";
    /// Notification, [`StatusParams`](super::StatusParams): an item of the
    /// status bar, set or removed.
    pub const STATUS: &str = "statusBar/set";
    /// Request, [`OpenFileParams`](super::OpenFileParams) → `null`: opens a
    /// file in the editor.
    pub const OPEN_FILE: &str = "editor/openFile";
    /// Request, no parameters → [`ActiveFile`](super::ActiveFile): the file
    /// in front, its text and selection.
    pub const ACTIVE_FILE: &str = "editor/activeFile";
    /// Request, [`SaveFileParams`](super::SaveFileParams) → `bool`: asks
    /// the user where to save a text, and saves it there (`false`: the
    /// user said no).
    pub const SAVE_FILE: &str = "window/saveFile";
    /// Request, [`DiffParams`](super::DiffParams) → `null`: shows two texts
    /// side by side.
    pub const OPEN_DIFF: &str = "editor/openDiff";
    /// Request, [`ReplaceTextParams`](super::ReplaceTextParams) → `null`:
    /// replaces the text of a file open in the editor, as one change the
    /// user can undo (an error if it is not open).
    pub const REPLACE_TEXT: &str = "editor/replaceText";
    /// Notification, [`PublishParams`](super::PublishParams): the problems
    /// the plugin found in a file, shown with the others (an empty list
    /// takes them away).
    pub const PUBLISH_DIAGNOSTICS: &str = "diagnostics/publish";
}

/// The errors of JSON-RPC, and the editor's own.
pub mod codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
    /// The user said no, or closed the question.
    pub const CANCELLED: i64 = -32800;
}

// ─── Parameters ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    /// The protocol version of the editor ([`API_VERSION`]).
    pub api: u32,
    /// The version of the editor (`1.28.0`).
    pub app_version: String,
    pub settings: Settings,
    /// The folders open as projects.
    #[serde(default)]
    pub workspace: Vec<PathBuf>,
    /// Where the plugin is installed (its `plugin.json` is there).
    pub plugin_dir: PathBuf,
    /// A folder of its own for the plugin to keep data in. It stays when
    /// the plugin is uninstalled, so a reinstall finds it again.
    pub data_dir: PathBuf,
}

/// What the plugin may want to follow of the editor's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// The language of the interface (`en`, `fr`).
    pub locale: String,
    /// The dark theme is on.
    pub dark: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandParams {
    /// The id of the command, as in the manifest.
    pub command: String,
    #[serde(default)]
    pub context: Context,
}

/// Where the user was when something happened.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// The file in front, if it is saved somewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
    /// The folders open as projects.
    #[serde(default)]
    pub workspace: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelParams {
    /// The id of the panel, as in the manifest.
    pub panel: String,
}

/// Something the user did in a panel. `action` is the `action` of a button,
/// or the `id` of the field, box, list or tree that changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiEvent {
    pub panel: String,
    pub action: String,
    /// What it holds now: the text of a field (`String`), a box (`bool`), the
    /// option picked (`String`), the id of the row of a list (`String`), or
    /// for a tree `{"item": "<id>", "kind": "click" | "toggle"}`. Absent for
    /// a button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

impl UiEvent {
    /// The value as text (fields, options, rows of a list).
    pub fn text(&self) -> Option<&str> {
        self.value.as_ref()?.as_str()
    }

    /// The value as a yes / no (check boxes).
    pub fn checked(&self) -> Option<bool> {
        self.value.as_ref()?.as_bool()
    }

    /// The row of a tree and whether it was clicked or opened / closed.
    pub fn tree_item(&self) -> Option<(&str, TreeAction)> {
        let value = self.value.as_ref()?;
        let item = value.get("item")?.as_str()?;
        let kind = match value.get("kind")?.as_str()? {
            "toggle" => TreeAction::Toggle,
            _ => TreeAction::Click,
        };
        Some((item, kind))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeAction {
    Click,
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileParams {
    pub path: PathBuf,
    /// What the file held before, when the editor knows it: a file it
    /// rewrote (a replace in files), one about to be deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
}

impl FileParams {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FileParams {
            path: path.into(),
            previous: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabParams {
    /// Its id, unique for the plugin: letters, digits, `.`, `-`, `_`.
    pub tab: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabDirtyParams {
    pub tab: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameParams {
    pub from: PathBuf,
    pub to: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveFileParams {
    /// The name offered (`changes.patch`).
    pub name: String,
    pub text: String,
}

/// The file in front: none for a tab that is not a saved file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveFileParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// A text and what it is called.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Named {
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffParams {
    pub left: Named,
    pub right: Named,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceTextParams {
    pub path: PathBuf,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishParams {
    pub path: PathBuf,
    #[serde(default)]
    pub diagnostics: Vec<crate::lint::Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderParams {
    pub panel: String,
    pub root: Node,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    #[default]
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifyParams {
    pub message: String,
    #[serde(default)]
    pub level: Level,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmParams {
    pub message: String,
    /// The label of the button that says yes (the editor's "OK" if absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

/// An item of the status bar. A plugin has as many as it wants, told apart
/// by their `id`; one without `text` is removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    /// A command of the plugin, run when the item is clicked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFileParams {
    pub path: PathBuf,
    /// The line to put the cursor on, counted from 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveFile {
    /// Absent for a file that was never saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// The text as it is in the editor (saved or not).
    pub text: String,
    /// The text selected, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
    /// The language the editor highlights it as (`rust`, `python`, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

// ─── Messages ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// A JSON-RPC message, either way.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request {
        id: i64,
        method: String,
        params: Value,
    },
    Response {
        id: i64,
        result: Result<Value, RpcError>,
    },
    Notification {
        method: String,
        params: Value,
    },
}

impl Message {
    pub fn request(id: i64, method: &str, params: impl Serialize) -> Self {
        Message::Request {
            id,
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap_or(Value::Null),
        }
    }

    pub fn notification(method: &str, params: impl Serialize) -> Self {
        Message::Notification {
            method: method.to_string(),
            params: serde_json::to_value(params).unwrap_or(Value::Null),
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Message::Request { id, method, params } => {
                json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
            }
            Message::Response { id, result: Ok(v) } => {
                json!({ "jsonrpc": "2.0", "id": id, "result": v })
            }
            Message::Response { id, result: Err(e) } => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": e.code, "message": e.message } })
            }
            Message::Notification { method, params } => {
                json!({ "jsonrpc": "2.0", "method": method, "params": params })
            }
        }
    }

    /// The message a JSON value is, or `None` if it is none of the three.
    pub fn from_value(value: Value) -> Option<Self> {
        let Value::Object(mut map) = value else {
            return None;
        };
        let id = map.get("id").and_then(Value::as_i64);
        let params = map.remove("params").unwrap_or(Value::Null);
        match (id, map.remove("method")) {
            (Some(id), Some(Value::String(method))) => {
                Some(Message::Request { id, method, params })
            }
            (None, Some(Value::String(method))) => Some(Message::Notification { method, params }),
            (Some(id), None) => {
                let result = match map.remove("error") {
                    Some(error) => Err(serde_json::from_value(error).unwrap_or(RpcError {
                        code: codes::INTERNAL_ERROR,
                        message: "error".into(),
                    })),
                    None => Ok(map.remove("result").unwrap_or(Value::Null)),
                };
                Some(Message::Response { id, result })
            }
            _ => None,
        }
    }
}

// ─── Framing ──────────────────────────────────────────────────────────────

/// Writes one message with its `Content-Length` header.
pub fn write_message(out: &mut impl Write, message: &Message) -> io::Result<()> {
    let body = message.to_value().to_string();
    write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
    out.write_all(body.as_bytes())?;
    out.flush()
}

/// Reads the next message. `Ok(None)` at the end of the input; a body that
/// is not a JSON-RPC message is skipped.
pub fn read_message(input: &mut impl BufRead) -> io::Result<Option<Message>> {
    loop {
        let mut length = None;
        loop {
            let mut line = String::new();
            if input.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let line = line.trim_end();
            if line.is_empty() {
                if length.is_some() {
                    break;
                }
                continue;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    length = value.trim().parse::<usize>().ok();
                }
            }
        }
        let length = length.unwrap_or(0);
        if length > MAX_MESSAGE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message too large",
            ));
        }
        let mut body = vec![0; length];
        input.read_exact(&mut body)?;
        if let Some(message) = serde_json::from_slice(&body)
            .ok()
            .and_then(Message::from_value)
        {
            return Ok(Some(message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_go_through_the_framing_unchanged() {
        let messages = vec![
            Message::request(1, methods::INITIALIZE, json!({"api": 1})),
            Message::notification(methods::NOTIFY, json!({"message": "é"})),
            Message::Response {
                id: 1,
                result: Ok(json!(true)),
            },
            Message::Response {
                id: 2,
                result: Err(RpcError {
                    code: codes::CANCELLED,
                    message: "no".into(),
                }),
            },
        ];
        let mut buffer = Vec::new();
        for m in &messages {
            write_message(&mut buffer, m).unwrap();
        }
        let mut input = io::BufReader::new(&buffer[..]);
        for m in &messages {
            assert_eq!(read_message(&mut input).unwrap().as_ref(), Some(m));
        }
        assert_eq!(read_message(&mut input).unwrap(), None);
    }

    #[test]
    fn a_body_that_is_not_a_message_is_skipped() {
        let mut raw = b"Content-Length: 4\r\n\r\n[1]\nContent-Length: 2\r\n\r\n{}".to_vec();
        write_message(&mut raw, &Message::notification("a", Value::Null)).unwrap();
        let mut input = io::BufReader::new(&raw[..]);
        assert_eq!(
            read_message(&mut input).unwrap(),
            Some(Message::notification("a", Value::Null))
        );
    }

    #[test]
    fn a_huge_message_is_refused() {
        let raw = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE + 1);
        let mut input = io::BufReader::new(raw.as_bytes());
        assert!(read_message(&mut input).is_err());
    }

    #[test]
    fn tree_events_say_the_row_and_what_happened() {
        let event = UiEvent {
            panel: "p".into(),
            action: "tree".into(),
            value: Some(json!({"item": "a/b", "kind": "toggle"})),
        };
        assert_eq!(event.tree_item(), Some(("a/b", TreeAction::Toggle)));
        assert_eq!(event.text(), None);
    }
}
