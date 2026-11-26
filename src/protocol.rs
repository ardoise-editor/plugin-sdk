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
    /// Request, [`FileParams`](super::FileParams) → [`DeleteOptions`](super::DeleteOptions),
    /// to the plugins that follow `file_deleted`: the boxes they add to the
    /// question before a file is deleted. The ones left ticked come back in
    /// the `options` of `event/fileDeleted`.
    pub const DELETE_OPTIONS: &str = "file/deleteOptions";
    /// Request, no parameters → `null`: the user removes the plugin with its
    /// data. What only the plugin knows of goes now (its passwords in the
    /// system keychain); the editor deletes its data folder after.
    pub const FORGET_DATA: &str = "data/forget";
    /// Notification, [`RenameParams`](super::RenameParams) (`file_renamed`).
    pub const FILE_RENAMED: &str = "event/fileRenamed";
    /// Notification, [`WorkspaceParams`](super::WorkspaceParams): the
    /// folders open as projects changed (`workspace_changed` in the
    /// manifest).
    pub const WORKSPACE_CHANGED: &str = "event/workspaceChanged";
    /// Notification, [`ActiveFileParams`](super::ActiveFileParams): the
    /// file in front changed (`active_file` in the manifest).
    pub const ACTIVE_FILE_CHANGED: &str = "event/activeFileChanged";
    /// Notification, [`PanelParams`](super::PanelParams) with the tab as
    /// `panel`: a tab of the plugin was closed.
    pub const TAB_CLOSED: &str = "tab/closed";
    /// Notification, [`PanelParams`](super::PanelParams): the user asked to
    /// save the tab (Ctrl/Cmd+S).
    pub const TAB_SAVE: &str = "tab/save";
    /// Request, [`CompletionParams`](super::CompletionParams) →
    /// [`Completions`](super::Completions): what may come at the cursor of
    /// an editor of a panel (one made `with_completion`), or of a file the
    /// plugin completes (`editor/setCompleter`): `panel` is then empty and
    /// `editor` the path of the file.
    pub const COMPLETION: &str = "completion/request";
    /// Notification, no parameters: the Plugins tab of Search Everywhere
    /// opened; the plugin may send its items again.
    pub const SEARCH_OPENED: &str = "search/opened";
    /// Notification, [`SearchPickedParams`](super::SearchPickedParams): an
    /// item of the plugin was picked in Search Everywhere.
    pub const SEARCH_PICKED: &str = "search/picked";
    /// Notification, [`DiffActionParams`](super::DiffActionParams): a button
    /// of a diff tab of the plugin.
    pub const DIFF_ACTION: &str = "diff/action";
    /// Notification, [`MergeParams`](super::MergeParams) with the text
    /// written: a conflict was resolved.
    pub const MERGE_RESOLVED: &str = "merge/resolved";
    /// Notification, [`LineParams`](super::LineParams): a line of the
    /// annotations of a file was clicked.
    pub const ANNOTATION_CLICKED: &str = "annotation/clicked";
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

    /// Notification, [`LineParams`](super::LineParams): a run mark of the
    /// gutter was clicked (see `editor/setRunMarks`).
    pub const RUN_MARK_CLICKED: &str = "runMark/clicked";

    // ── From the plugin to the editor ──────────────────────────────────
    /// Notification, [`RunMarksParams`](super::RunMarksParams): the lines of
    /// a file that have a run mark (▶) in the gutter (a service of a compose
    /// file…). It replaces what the plugin said before for that file; no
    /// line takes the marks away. An editor older than 2.10 ignores it.
    pub const SET_RUN_MARKS: &str = "editor/setRunMarks";
    /// Notification, [`RenderParams`](super::RenderParams): what a panel
    /// shows, whole.
    pub const RENDER: &str = "ui/render";
    /// Request, [`TabParams`](super::TabParams) → `null`: opens a tab of the
    /// plugin among the files (or brings it in front). It shows what the
    /// plugin renders for the panel named as the tab; after a restart, the
    /// editor sends `panel/opened` for it when it is first shown. With
    /// `dialog` it opens as a window over the editor instead.
    pub const OPEN_TAB: &str = "editor/openTab";
    /// Notification, [`TabDirtyParams`](super::TabDirtyParams): the tab
    /// holds changes not kept (its name is marked, closing it asks first).
    pub const TAB_DIRTY: &str = "editor/tabDirty";
    /// Notification, [`PanelParams`](super::PanelParams): closes a tab.
    pub const CLOSE_TAB: &str = "editor/closeTab";
    /// Notification, [`DecorateParams`](super::DecorateParams): how files
    /// stand (their color in the tree and on the tabs). It replaces what
    /// the plugin said before.
    pub const DECORATE: &str = "editor/decorate";
    /// Notification, [`SetBaseParams`](super::SetBaseParams): the text a
    /// file is compared with to mark its changed lines (what the last commit
    /// has of it); the editor marks them as it is typed in.
    pub const SET_BASE: &str = "editor/setBase";
    /// Notification, [`SetOriginParams`](super::SetOriginParams): where a
    /// file comes from (`host:/path` of a copy), shown in place of its
    /// folder; without `origin`, its folder again.
    pub const SET_ORIGIN: &str = "editor/setOrigin";
    /// Notification, [`AnnotateParams`](super::AnnotateParams): a column of
    /// text left of the line numbers of a file (a blame), or none.
    pub const ANNOTATE: &str = "editor/annotate";
    /// Notification, [`CompleterParams`](super::CompleterParams): the
    /// plugin completes what is typed in a file (a Dockerfile…): the editor
    /// asks it with `completion/request`. An editor older than 2.10 ignores
    /// it.
    pub const SET_COMPLETER: &str = "editor/setCompleter";
    /// Request, [`DiffTabParams`](super::DiffTabParams) → `null`: shows the
    /// diffs of files in a tab of the plugin (or replaces what a tab with
    /// that id shows). Its buttons send `diff/action`.
    pub const OPEN_DIFF_TAB: &str = "editor/openDiffTab";
    /// Request, [`PickSavePathParams`](super::PickSavePathParams) → a path
    /// or `null`: where to save a file, chosen by the user; the plugin
    /// writes it (a file too big to go through the editor).
    pub const PICK_SAVE_PATH: &str = "window/pickSavePath";
    /// Request, [`EditorStateParams`](super::EditorStateParams) →
    /// [`EditorState`](super::EditorState): the text of an editor of a
    /// panel, where its cursor is and what is selected.
    pub const EDITOR_STATE: &str = "panel/editorState";
    /// Notification, [`SearchItemsParams`](super::SearchItemsParams): what
    /// the plugin offers in the Plugins tab of Search Everywhere (branches,
    /// commits…). It replaces what it offered before.
    pub const SEARCH_ITEMS: &str = "editor/searchItems";
    /// Request, [`OpenTextParams`](super::OpenTextParams) → the path of the
    /// tab (to annotate it): a text in a tab of its own, to read only (a
    /// file as a commit had it).
    pub const OPEN_TEXT: &str = "editor/openText";
    /// Request, [`MergeParams`](super::MergeParams) → `null`: a file in
    /// conflict, to resolve in a tab. Once it is resolved the editor writes
    /// it and sends `merge/resolved`.
    pub const OPEN_MERGE: &str = "editor/openMerge";
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
    /// Request, [`PickFileParams`](super::PickFileParams) → path or `null`:
    /// asks the user for a file.
    pub const PICK_FILE: &str = "window/pickFile";
    /// Notification, [`CopyParams`](super::CopyParams): puts a text on the
    /// clipboard.
    pub const COPY: &str = "window/copy";
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

    /// The row of a tree whose context menu sent this (its action is the
    /// item's).
    pub fn menu_item(&self) -> Option<&str> {
        let value = self.value.as_ref()?;
        if value.get("kind").is_some() {
            return None;
        }
        value.get("item")?.as_str()
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
    /// For a file deleted: the ids of the boxes of this plugin the user left
    /// ticked (see `file/deleteOptions`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

impl FileParams {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        FileParams {
            path: path.into(),
            previous: None,
            options: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceParams {
    #[serde(default)]
    pub folders: Vec<PathBuf>,
}

/// A box a plugin adds to the question before a file is deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteOption {
    pub id: String,
    pub label: String,
    /// Ticked at first.
    #[serde(default)]
    pub checked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteOptions {
    #[serde(default)]
    pub options: Vec<DeleteOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabParams {
    /// Its id, unique for the plugin: letters, digits, `.`, `-`, `_`.
    pub tab: String,
    pub title: String,
    /// Opens it as a dialog (a window over the editor, with the title, a
    /// cross and Escape to close it) instead of a tab among the files. The
    /// plugin hears `tab/closed` when it is closed, as for a tab.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dialog: bool,
    /// With `dialog`: the picto of its header (a Lucide icon name, as for a
    /// button) and the line under its title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabDirtyParams {
    pub tab: String,
    pub dirty: bool,
}

/// How a file stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Modified,
    Added,
    Untracked,
    Conflicted,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDecoration {
    pub path: PathBuf,
    pub state: FileState,
    /// A folder (an ignored folder covers what is in it).
    #[serde(default)]
    pub folder: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecorateParams {
    pub files: Vec<FileDecoration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetOriginParams {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetBaseParams {
    pub path: PathBuf,
    /// What it is compared with (none: the file is new to it, every line
    /// counts as added).
    #[serde(default)]
    pub text: Option<String>,
    /// The file is outside of what the plugin knows: no marks at all.
    #[serde(default)]
    pub none: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMarksParams {
    pub path: PathBuf,
    /// From 0.
    #[serde(default)]
    pub lines: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleterParams {
    pub path: PathBuf,
    /// The plugin completes the file (`false`: no more).
    #[serde(default)]
    pub on: bool,
    /// Typing one of these asks for the list (typing a name does too).
    #[serde(default)]
    pub triggers: Vec<String>,
}

/// A line of a column of annotations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationLine {
    pub text: String,
    /// The tint behind it (`[r, g, b]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tint: Option<[u8; 3]>,
    /// Which entry of the details a pointer resting on it shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotations {
    /// One per line of the file.
    pub lines: Vec<AnnotationLine>,
    /// What a card shows, as lines: the first is drawn as a link.
    #[serde(default)]
    pub details: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotateParams {
    pub path: PathBuf,
    /// None: the column goes.
    #[serde(default)]
    pub annotations: Option<Annotations>,
}

/// What a diff is of: it decides the buttons of its blocks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    /// Only to read.
    #[default]
    Plain,
    /// Changes not staged: each block can be staged, or put back.
    Unstaged,
    /// Changes staged: each block can be unstaged.
    Staged,
    /// A commit (or another revision) against its parent; it can be turned
    /// into the revision against the file now.
    Commit,
    /// A revision against the file now: each block can be put back as the
    /// revision had it; it can be turned back into the commit's own diff.
    Against,
}

/// The diff of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFile {
    /// Its path as shown (from the top of the repository).
    pub path: String,
    /// Its path before a rename.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// The diff, as `git diff` writes it (one file).
    pub patch: String,
    #[serde(default)]
    pub mode: DiffMode,
    /// The file on the disk (to open it from the diff).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffTabParams {
    pub tab: String,
    pub title: String,
    pub files: Vec<DiffFile>,
    /// The file shown first.
    #[serde(default)]
    pub start: usize,
}

/// What a button of a diff tab asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffAction {
    /// Apply this patch to the index (`git apply --cached`), reversed or not:
    /// a block or the lines picked, staged or unstaged.
    ApplyToIndex { patch: String, reverse: bool },
    /// Put back, in the file, what it had before this block.
    Rollback { block: usize },
    /// Blanks left out of the diff or not, and the lines of context: the
    /// plugin sends the diffs again.
    Options { ignore_blanks: bool, context: u32 },
    /// A commit against the file now, or against its parent.
    ToggleCompare,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffActionParams {
    pub tab: String,
    /// The file of the tab it is about.
    pub file: usize,
    pub action: DiffAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeParams {
    pub tab: String,
    /// The file in conflict.
    pub path: PathBuf,
    /// Its text with the conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`);
    /// in `merge/resolved`, the text written.
    pub text: String,
    /// The side after `=======` is the user's own (a rebase replays the
    /// user's commits onto the other side).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub swap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineParams {
    pub path: PathBuf,
    /// From 0.
    pub line: usize,
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
    /// Bytes to save instead of the text (a binary file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickFileParams {
    #[serde(default)]
    pub title: String,
    /// The extensions offered (none: any file).
    #[serde(default)]
    pub extensions: Vec<String>,
    /// A folder is asked instead of a file (Ardoise 2.11; before, a file).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyParams {
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
    /// The lines of the selection, or the line of the cursor: first and
    /// last, from 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickSavePathParams {
    /// The name offered (`orders.csv`).
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorStateParams {
    pub panel: String,
    /// The `id` of the editor.
    pub editor: String,
}

/// A place in a text: line and column (characters), from 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPosition {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorState {
    pub text: String,
    pub cursor: TextPosition,
    /// The other end of the selection, if something is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<TextPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionParams {
    pub panel: String,
    /// The `id` of the editor.
    pub editor: String,
    pub text: String,
    /// Where the cursor is, in characters from the start.
    pub offset: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    /// What is put in place of the word at the cursor (none: the label).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert: Option<String>,
    /// Shown dimmed beside it (a type, a table).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completions {
    pub items: Vec<CompletionItem>,
    /// Where the replaced word starts, in characters (none: the word made
    /// of letters, digits and `_` before the cursor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<usize>,
}

/// Something the plugin offers in Search Everywhere.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchItem {
    /// What `search/picked` gives back.
    pub id: String,
    pub label: String,
    /// Shown dimmed beside it; searched too.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// A Nerd Font glyph.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchItemsParams {
    pub items: Vec<SearchItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPickedParams {
    pub id: String,
}

/// A text to show in a tab of its own, to read only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenTextParams {
    pub title: String,
    pub text: String,
    /// The extension its colors are chosen by (`rs`), if any.
    #[serde(default)]
    pub extension: String,
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
    #[test]
    fn a_tab_is_a_dialog_only_when_it_says_so() {
        use super::TabParams;
        let old: TabParams = serde_json::from_str(r#"{"tab":"a","title":"A"}"#).unwrap();
        assert!(!old.dialog);
        assert!(serde_json::to_value(&old).unwrap().get("dialog").is_none());
        assert!(serde_json::to_value(&old).unwrap().get("icon").is_none());
        let dialog = TabParams {
            dialog: true,
            icon: Some("database".into()),
            subtitle: Some("PostgreSQL, MySQL or SQLite".into()),
            ..old
        };
        let json = serde_json::to_value(&dialog).unwrap();
        assert_eq!(json["dialog"], true);
        assert_eq!(json["icon"], "database");
        assert_eq!(json["subtitle"], "PostgreSQL, MySQL or SQLite");

        assert_eq!(serde_json::from_value::<TabParams>(json).unwrap(), dialog);
    }

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

    #[test]
    fn a_tree_menu_event_is_told_from_a_click() {
        let menu = UiEvent {
            panel: "p".into(),
            action: "rename".into(),
            value: Some(serde_json::json!({"item": "a"})),
        };
        assert_eq!(menu.menu_item(), Some("a"));
        assert_eq!(menu.tree_item(), None);
        let click = UiEvent {
            value: Some(serde_json::json!({"item": "a", "kind": "click"})),
            ..menu
        };
        assert_eq!(click.menu_item(), None);
        assert!(click.tree_item().is_some());
        let origin = serde_json::to_value(SetOriginParams {
            path: "/tmp/x".into(),
            origin: None,
        })
        .unwrap();
        assert!(origin.get("origin").is_none());
    }
}
