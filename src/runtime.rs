//! The plugin side: implement [`Plugin`] and hand it to [`run`].
//!
//! ```no_run
//! use ardoise_plugin::{run, Host, Plugin, ui::*};
//!
//! struct Hello;
//!
//! impl Plugin for Hello {
//!     fn command(&mut self, host: &Host, command: &str, _: &ardoise_plugin::protocol::Context) {
//!         if command == "hello.greet" {
//!             host.notify("Hello!");
//!         }
//!     }
//!     fn panel_opened(&mut self, host: &Host, panel: &str) {
//!         host.render(panel, text("Hello from the plugin"));
//!     }
//! }
//!
//! fn main() {
//!     run(Hello).unwrap();
//! }
//! ```
//!
//! [`run`] reads the editor's messages on the standard input and calls the
//! methods of the plugin one at a time, on the thread that called it. A
//! [`Host`] talks back. The standard output belongs to the protocol: print
//! with `eprintln!` (it goes to the plugin's log), never `println!`.

use std::collections::HashMap;
use std::io::{self, BufReader, Write};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::lint::{FixParams, FixResult, LintParams, LintResult};
use crate::protocol::{
    codes, methods, read_message, write_message, ActiveFile, ActiveFileParams, AnnotateParams,
    Annotations, CommandParams, CompletionParams, Completions, ConfirmParams, Context, CopyParams,
    DecorateParams, DeleteOption, DeleteOptions, DiffActionParams, DiffParams, DiffTabParams,
    EditorState, EditorStateParams, FileDecoration, FileParams, InitializeParams, Level,
    LineParams, MergeParams, Message, Named, NotifyParams, OpenFileParams, OpenTextParams,
    PanelParams, PickFileParams, PickSavePathParams, PublishParams, RenameParams, RenderParams,
    ReplaceTextParams, RpcError, SaveFileParams, SearchItem, SearchItemsParams, SearchPickedParams,
    SetBaseParams, SetOriginParams, Settings, StatusParams, TabDirtyParams, TabParams, UiEvent,
};
use crate::spell::{
    CheckParams, CheckResult, Language as SpellLanguage, LearnParams, SuggestParams,
};
use crate::ui::Node;

/// What the plugin does when the editor says something. Every method has a
/// default that does nothing.
#[allow(unused_variables)]
pub trait Plugin {
    /// The first call, before any other.
    fn initialize(&mut self, host: &Host, params: &InitializeParams) {}
    /// A command of the manifest was run.
    fn command(&mut self, host: &Host, command: &str, context: &Context) {}
    /// A panel was opened: send what it shows with [`Host::render`].
    fn panel_opened(&mut self, host: &Host, panel: &str) {}
    fn panel_closed(&mut self, host: &Host, panel: &str) {}
    /// The Plugins tab of Search Everywhere opened (see
    /// [`Host::search_items`]).
    fn search_opened(&mut self, host: &Host) {}
    /// An item of the plugin was picked in Search Everywhere.
    fn search_picked(&mut self, host: &Host, id: &str) {}
    /// A button of a diff tab of the plugin.
    fn diff_action(&mut self, host: &Host, params: &DiffActionParams) {}
    /// A file in conflict was resolved (the text written).
    fn merge_resolved(&mut self, host: &Host, params: &MergeParams) {}
    /// A line of the annotations of a file was clicked (from 0).
    fn annotation_clicked(&mut self, host: &Host, path: &Path, line: usize) {}
    /// A tab of the plugin was closed.
    fn tab_closed(&mut self, host: &Host, tab: &str) {}
    /// The user asked to save a tab of the plugin.
    fn tab_save(&mut self, host: &Host, tab: &str) {}
    /// The user did something in a panel.
    fn ui_event(&mut self, host: &Host, event: &UiEvent) {}
    fn file_opened(&mut self, host: &Host, path: &Path) {}
    fn file_saved(&mut self, host: &Host, path: &Path) {}
    /// The same, with what the file held before when the editor knows it
    /// (a replace in files); by default, [`Plugin::file_saved`].
    fn file_saved_from(&mut self, host: &Host, path: &Path, previous: Option<&str>) {
        self.file_saved(host, path);
    }
    fn file_closed(&mut self, host: &Host, path: &Path) {}
    /// A file is about to be deleted (`file_deleted`), with what it held.
    fn file_deleted(&mut self, host: &Host, path: &Path, text: Option<&str>) {}
    /// `file/deleteOptions`: the boxes the plugin adds to the question
    /// before `path` is deleted (none by default).
    fn delete_options(&mut self, host: &Host, path: &Path) -> Vec<DeleteOption> {
        Vec::new()
    }
    /// The boxes of the plugin the user left ticked for a file deleted
    /// (after `file_deleted`).
    fn delete_chosen(&mut self, host: &Host, path: &Path, options: &[String]) {}
    /// A file or folder was renamed or moved (`file_renamed`).
    fn file_renamed(&mut self, host: &Host, from: &Path, to: &Path) {}
    fn settings_changed(&mut self, host: &Host, settings: &Settings) {}
    /// The file in front changed (`active_file` in the manifest).
    fn active_file_changed(&mut self, host: &Host, path: Option<&Path>) {}
    /// `completion/request`: what may come at the cursor of an editor of a
    /// panel made `with_completion`.
    fn completion(
        &mut self,
        host: &Host,
        params: &CompletionParams,
    ) -> Result<Completions, String> {
        Ok(Completions::default())
    }
    /// `lint/run`, for a plugin with `contributes.linter`.
    fn lint(&mut self, host: &Host, params: &LintParams) -> Result<LintResult, String> {
        Err(format!("{} is not supported", methods::LINT_RUN))
    }
    /// `lint/fix`.
    fn fix(&mut self, host: &Host, params: &FixParams) -> Result<FixResult, String> {
        Err(format!("{} is not supported", methods::LINT_FIX))
    }
    /// `spell/languages`, for a plugin with `contributes.speller`.
    fn spell_languages(&mut self, host: &Host) -> Result<Vec<SpellLanguage>, String> {
        Err(format!("{} is not supported", methods::SPELL_LANGUAGES))
    }
    /// `spell/check`.
    fn spell_check(&mut self, host: &Host, params: &CheckParams) -> Result<CheckResult, String> {
        Err(format!("{} is not supported", methods::SPELL_CHECK))
    }
    /// `spell/suggest`.
    fn spell_suggest(
        &mut self,
        host: &Host,
        params: &SuggestParams,
    ) -> Result<Vec<String>, String> {
        Err(format!("{} is not supported", methods::SPELL_SUGGEST))
    }
    /// `spell/learn`.
    fn spell_learn(&mut self, host: &Host, params: &LearnParams) -> Result<(), String> {
        Err(format!("{} is not supported", methods::SPELL_LEARN))
    }
    /// Any other request of the editor: answer it.
    fn request(&mut self, host: &Host, method: &str, params: Value) -> Result<Value, RpcError> {
        Err(RpcError {
            code: codes::METHOD_NOT_FOUND,
            message: method.to_string(),
        })
    }
    /// `data/forget`: the user removes the plugin with its data. Delete what
    /// only the plugin knows of, outside of its data folder (passwords in
    /// the system keychain); the editor deletes that folder afterwards.
    fn forget_data(&mut self, host: &Host) -> Result<(), String> {
        Ok(())
    }
    /// The last call: the editor is closing or the plugin is uninstalled.
    fn shutdown(&mut self, host: &Host) {}
}

type Pending = Arc<Mutex<HashMap<i64, Sender<Result<Value, RpcError>>>>>;

/// How the plugin talks to the editor. Cheap to clone, usable from any
/// thread (a plugin may render from a thread of its own).
#[derive(Clone)]
pub struct Host {
    out: Arc<Mutex<Box<dyn Write + Send>>>,
    pending: Pending,
    next_id: Arc<Mutex<i64>>,
}

/// How long [`Host`] waits for the editor's answer to a request.
const ANSWER_WAIT: Duration = Duration::from_secs(300);

impl Host {
    /// A host that writes what the plugin says to `out`, for the tests of a
    /// plugin: notifications land there whole (read them back with
    /// [`read_message`](crate::protocol::read_message)). Only for
    /// notifications: a request would wait for an answer that never comes.
    pub fn to_writer(out: impl Write + Send + 'static) -> Host {
        Host {
            out: Arc::new(Mutex::new(Box::new(out))),
            pending: Arc::default(),
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    fn send(&self, message: &Message) -> io::Result<()> {
        let mut out = self
            .out
            .lock()
            .map_err(|_| io::Error::other("output poisoned"))?;
        write_message(&mut *out, message)
    }

    /// Sends a notification. Errors (the editor is gone) are ignored: the
    /// plugin is about to be stopped anyway.
    pub fn notification(&self, method: &str, params: impl Serialize) {
        let _ = self.send(&Message::notification(method, params));
    }

    /// Sends a request and waits for the answer.
    pub fn request<T: DeserializeOwned>(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<T, RpcError> {
        let id = {
            let mut next = self.next_id.lock().map_err(|_| internal("poisoned"))?;
            *next += 1;
            *next
        };
        let (tx, rx) = channel();
        self.pending
            .lock()
            .map_err(|_| internal("poisoned"))?
            .insert(id, tx);
        self.send(&Message::request(id, method, params))
            .map_err(|e| internal(&e.to_string()))?;
        let answer = rx
            .recv_timeout(ANSWER_WAIT)
            .map_err(|_| internal("the editor did not answer"));
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&id);
        }
        serde_json::from_value(answer??).map_err(|e| RpcError {
            code: codes::INVALID_PARAMS,
            message: e.to_string(),
        })
    }

    /// Replaces what a panel shows.
    pub fn render(&self, panel: &str, root: Node) {
        self.notification(
            methods::RENDER,
            RenderParams {
                panel: panel.to_string(),
                root,
            },
        );
    }

    /// Where to save a file, chosen by the user (none: given up); the
    /// plugin writes it.
    pub fn pick_save_path(
        &self,
        name: impl Into<String>,
    ) -> Result<Option<std::path::PathBuf>, RpcError> {
        self.request(
            methods::PICK_SAVE_PATH,
            PickSavePathParams { name: name.into() },
        )
    }

    /// The text of an editor of a panel, its cursor and its selection.
    pub fn editor_state(&self, panel: &str, editor: &str) -> Result<EditorState, RpcError> {
        self.request(
            methods::EDITOR_STATE,
            EditorStateParams {
                panel: panel.to_string(),
                editor: editor.to_string(),
            },
        )
    }

    /// A text in a tab of its own, to read only; the path of that tab.
    pub fn open_text(
        &self,
        title: impl Into<String>,
        text: impl Into<String>,
        extension: impl Into<String>,
    ) -> Result<std::path::PathBuf, RpcError> {
        self.request(
            methods::OPEN_TEXT,
            OpenTextParams {
                title: title.into(),
                text: text.into(),
                extension: extension.into(),
            },
        )
    }

    /// Shows diffs in a tab of the plugin (or replaces what it shows).
    pub fn open_diff_tab(&self, params: DiffTabParams) -> Result<(), RpcError> {
        self.request::<Value>(methods::OPEN_DIFF_TAB, params)
            .map(|_| ())
    }

    /// A file in conflict, to resolve in a tab (`swap`: the side after
    /// `=======` is the user's, as in a rebase).
    pub fn open_merge(
        &self,
        tab: &str,
        path: impl AsRef<Path>,
        text: impl Into<String>,
        swap: bool,
    ) -> Result<(), RpcError> {
        self.request::<Value>(
            methods::OPEN_MERGE,
            MergeParams {
                tab: tab.to_string(),
                path: path.as_ref().to_path_buf(),
                text: text.into(),
                swap,
            },
        )
        .map(|_| ())
    }

    /// What the plugin offers in Search Everywhere (replaces what it
    /// offered before).
    pub fn search_items(&self, items: Vec<SearchItem>) {
        self.notification(methods::SEARCH_ITEMS, SearchItemsParams { items });
    }

    /// How files stand (replaces what was said before).
    pub fn decorate(&self, files: Vec<FileDecoration>) {
        self.notification(methods::DECORATE, DecorateParams { files });
    }

    /// The text a file is compared with to mark its changed lines (none:
    /// new, every line added).
    /// Where a file comes from, shown in place of its folder (none: its
    /// folder again).
    pub fn set_origin(&self, path: impl AsRef<Path>, origin: Option<String>) {
        self.notification(
            methods::SET_ORIGIN,
            SetOriginParams {
                path: path.as_ref().to_path_buf(),
                origin,
            },
        );
    }

    pub fn set_base(&self, path: impl AsRef<Path>, text: Option<String>) {
        self.notification(
            methods::SET_BASE,
            SetBaseParams {
                path: path.as_ref().to_path_buf(),
                text,
                none: false,
            },
        );
    }

    /// No marks of changes for a file.
    pub fn clear_base(&self, path: impl AsRef<Path>) {
        self.notification(
            methods::SET_BASE,
            SetBaseParams {
                path: path.as_ref().to_path_buf(),
                text: None,
                none: true,
            },
        );
    }

    /// A column of annotations left of the line numbers of a file (none:
    /// it goes).
    pub fn annotate(&self, path: impl AsRef<Path>, annotations: Option<Annotations>) {
        self.notification(
            methods::ANNOTATE,
            AnnotateParams {
                path: path.as_ref().to_path_buf(),
                annotations,
            },
        );
    }

    /// Opens a tab of the plugin among the files (see
    /// [`methods::OPEN_TAB`]); it shows what is rendered for `tab`.
    pub fn open_tab(&self, tab: &str, title: impl Into<String>) -> Result<(), RpcError> {
        self.request::<Value>(
            methods::OPEN_TAB,
            TabParams {
                tab: tab.to_string(),
                title: title.into(),
            },
        )
        .map(|_| ())
    }

    /// Says whether a tab holds changes not kept.
    pub fn tab_dirty(&self, tab: &str, dirty: bool) {
        self.notification(
            methods::TAB_DIRTY,
            TabDirtyParams {
                tab: tab.to_string(),
                dirty,
            },
        );
    }

    /// Closes a tab of the plugin.
    pub fn close_tab(&self, tab: &str) {
        self.notification(
            methods::CLOSE_TAB,
            PanelParams {
                panel: tab.to_string(),
            },
        );
    }

    /// Opens a panel of the plugin, if it is not open.
    pub fn show_panel(&self, panel: &str) {
        self.notification(
            methods::SHOW_PANEL,
            PanelParams {
                panel: panel.to_string(),
            },
        );
    }

    /// A message in the status bar.
    pub fn notify(&self, message: impl Into<String>) {
        self.notification(
            methods::NOTIFY,
            NotifyParams {
                message: message.into(),
                level: Level::Info,
            },
        );
    }

    /// An error in the status bar.
    pub fn error(&self, message: impl Into<String>) {
        self.notification(
            methods::NOTIFY,
            NotifyParams {
                message: message.into(),
                level: Level::Error,
            },
        );
    }

    /// Sets an item of the status bar (see [`StatusParams`]).
    pub fn status(&self, item: StatusParams) {
        self.notification(methods::STATUS, item);
    }

    /// Removes an item of the status bar.
    pub fn clear_status(&self, id: &str) {
        self.status(StatusParams {
            id: id.to_string(),
            text: None,
            tooltip: None,
            command: None,
        });
    }

    /// Asks the user; `Ok(false)` if they said no.
    pub fn confirm(&self, message: impl Into<String>) -> Result<bool, RpcError> {
        self.request(
            methods::CONFIRM,
            ConfirmParams {
                message: message.into(),
                confirm: None,
            },
        )
    }

    /// Opens a file in the editor, at a line (from 1) if given.
    pub fn open_file(&self, path: impl AsRef<Path>, line: Option<usize>) -> Result<(), RpcError> {
        self.request::<Value>(
            methods::OPEN_FILE,
            OpenFileParams {
                path: path.as_ref().to_path_buf(),
                line,
            },
        )
        .map(|_| ())
    }

    /// The file in front of the user.
    pub fn active_file(&self) -> Result<ActiveFile, RpcError> {
        self.request(methods::ACTIVE_FILE, Value::Null)
    }

    /// Asks the user where to save `text` (`name` offered); `Ok(false)` if
    /// they said no.
    pub fn save_file(
        &self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<bool, RpcError> {
        self.request(
            methods::SAVE_FILE,
            SaveFileParams {
                name: name.into(),
                text: text.into(),
                bytes: None,
            },
        )
    }

    /// Asks the user where to save `bytes`; `Ok(false)` if they said no.
    pub fn save_bytes(&self, name: impl Into<String>, bytes: Vec<u8>) -> Result<bool, RpcError> {
        self.request(
            methods::SAVE_FILE,
            SaveFileParams {
                name: name.into(),
                text: String::new(),
                bytes: Some(bytes),
            },
        )
    }

    /// Asks the user for a file (of these extensions; none: any).
    pub fn pick_file(
        &self,
        title: impl Into<String>,
        extensions: &[&str],
    ) -> Result<Option<std::path::PathBuf>, RpcError> {
        self.request(
            methods::PICK_FILE,
            PickFileParams {
                title: title.into(),
                extensions: extensions.iter().map(|e| e.to_string()).collect(),
            },
        )
    }

    /// Puts a text on the clipboard.
    pub fn copy(&self, text: impl Into<String>) {
        self.notification(methods::COPY, CopyParams { text: text.into() });
    }

    /// Shows two texts side by side: (title, text) each.
    pub fn open_diff(
        &self,
        left: (impl Into<String>, impl Into<String>),
        right: (impl Into<String>, impl Into<String>),
    ) -> Result<(), RpcError> {
        let named = |(title, text): (String, String)| Named { title, text };
        self.request::<Value>(
            methods::OPEN_DIFF,
            DiffParams {
                left: named((left.0.into(), left.1.into())),
                right: named((right.0.into(), right.1.into())),
            },
        )
        .map(|_| ())
    }

    /// Replaces the text of a file open in the editor (the user can undo it).
    pub fn replace_text(
        &self,
        path: impl AsRef<Path>,
        text: impl Into<String>,
    ) -> Result<(), RpcError> {
        self.request::<Value>(
            methods::REPLACE_TEXT,
            ReplaceTextParams {
                path: path.as_ref().to_path_buf(),
                text: text.into(),
            },
        )
        .map(|_| ())
    }

    /// The problems found in a file (none: those said before go).
    pub fn publish_diagnostics(
        &self,
        path: impl AsRef<Path>,
        diagnostics: Vec<crate::lint::Diagnostic>,
    ) {
        self.notification(
            methods::PUBLISH_DIAGNOSTICS,
            PublishParams {
                path: path.as_ref().to_path_buf(),
                diagnostics,
            },
        );
    }

    /// A line in the plugin's log (its standard error).
    pub fn log(&self, line: impl std::fmt::Display) {
        eprintln!("{line}");
    }
}

fn internal(message: &str) -> RpcError {
    RpcError {
        code: codes::INTERNAL_ERROR,
        message: message.to_string(),
    }
}

/// Runs the plugin on the standard input and output until the editor stops
/// it (or closes the input).
pub fn run(plugin: impl Plugin) -> io::Result<()> {
    run_on(BufReader::new(io::stdin()), Box::new(io::stdout()), plugin)
}

/// [`run`] on other streams (for tests).
pub fn run_on(
    input: impl io::BufRead + Send + 'static,
    output: Box<dyn Write + Send>,
    mut plugin: impl Plugin,
) -> io::Result<()> {
    let host = Host {
        out: Arc::new(Mutex::new(output)),
        pending: Arc::default(),
        next_id: Arc::new(Mutex::new(0)),
    };
    let incoming = read_in_background(input, host.pending.clone());
    for message in incoming {
        match message {
            Message::Request { id, method, params } => {
                let result = match method.as_str() {
                    methods::INITIALIZE => match serde_json::from_value(params) {
                        Ok(params) => {
                            plugin.initialize(&host, &params);
                            Ok(Value::Null)
                        }
                        Err(e) => Err(RpcError {
                            code: codes::INVALID_PARAMS,
                            message: e.to_string(),
                        }),
                    },
                    methods::SHUTDOWN => {
                        plugin.shutdown(&host);
                        host.send(&Message::Response {
                            id,
                            result: Ok(Value::Null),
                        })?;
                        return Ok(());
                    }
                    methods::LINT_RUN => typed(params, |p| plugin.lint(&host, &p)),
                    methods::COMPLETION => typed(params, |p| plugin.completion(&host, &p)),
                    methods::LINT_FIX => typed(params, |p| plugin.fix(&host, &p)),
                    methods::SPELL_LANGUAGES => {
                        typed(Value::Null, |()| plugin.spell_languages(&host))
                    }
                    methods::SPELL_CHECK => typed(params, |p| plugin.spell_check(&host, &p)),
                    methods::FORGET_DATA => typed(Value::Null, |()| {
                        plugin.forget_data(&host).map(|()| Value::Null)
                    }),
                    methods::DELETE_OPTIONS => typed(params, |p: FileParams| {
                        Ok(DeleteOptions {
                            options: plugin.delete_options(&host, &p.path),
                        })
                    }),
                    methods::SPELL_SUGGEST => typed(params, |p| plugin.spell_suggest(&host, &p)),
                    methods::SPELL_LEARN => typed(params, |p| plugin.spell_learn(&host, &p)),
                    _ => plugin.request(&host, &method, params),
                };
                host.send(&Message::Response { id, result })?;
            }
            Message::Notification { method, params } => {
                dispatch(&mut plugin, &host, &method, params);
            }
            // Answers are routed by the reader.
            Message::Response { .. } => {}
        }
    }
    plugin.shutdown(&host);
    Ok(())
}

/// Reads the parameters of a request and answers with what `f` made of them.
fn typed<P: DeserializeOwned, R: Serialize>(
    params: Value,
    f: impl FnOnce(P) -> Result<R, String>,
) -> Result<Value, RpcError> {
    let params = serde_json::from_value(params).map_err(|e| RpcError {
        code: codes::INVALID_PARAMS,
        message: e.to_string(),
    })?;
    let result = f(params).map_err(|message| RpcError {
        code: codes::INTERNAL_ERROR,
        message,
    })?;
    serde_json::to_value(result).map_err(|e| internal(&e.to_string()))
}

fn dispatch(plugin: &mut impl Plugin, host: &Host, method: &str, params: Value) {
    fn parse<T: DeserializeOwned>(params: Value) -> Option<T> {
        serde_json::from_value(params).ok()
    }
    match method {
        methods::COMMAND => {
            if let Some(p) = parse::<CommandParams>(params) {
                plugin.command(host, &p.command, &p.context);
            }
        }
        methods::PANEL_OPENED => {
            if let Some(p) = parse::<PanelParams>(params) {
                plugin.panel_opened(host, &p.panel);
            }
        }
        methods::PANEL_CLOSED => {
            if let Some(p) = parse::<PanelParams>(params) {
                plugin.panel_closed(host, &p.panel);
            }
        }
        methods::DIFF_ACTION => {
            if let Some(p) = parse::<DiffActionParams>(params) {
                plugin.diff_action(host, &p);
            }
        }
        methods::MERGE_RESOLVED => {
            if let Some(p) = parse::<MergeParams>(params) {
                plugin.merge_resolved(host, &p);
            }
        }
        methods::SEARCH_OPENED => plugin.search_opened(host),
        methods::SEARCH_PICKED => {
            if let Some(p) = parse::<SearchPickedParams>(params) {
                plugin.search_picked(host, &p.id);
            }
        }
        methods::ANNOTATION_CLICKED => {
            if let Some(p) = parse::<LineParams>(params) {
                plugin.annotation_clicked(host, &p.path, p.line);
            }
        }
        methods::TAB_CLOSED => {
            if let Some(p) = parse::<PanelParams>(params) {
                plugin.tab_closed(host, &p.panel);
            }
        }
        methods::TAB_SAVE => {
            if let Some(p) = parse::<PanelParams>(params) {
                plugin.tab_save(host, &p.panel);
            }
        }
        methods::UI_EVENT => {
            if let Some(event) = parse::<UiEvent>(params) {
                plugin.ui_event(host, &event);
            }
        }
        methods::FILE_OPENED => {
            if let Some(p) = parse::<FileParams>(params) {
                plugin.file_opened(host, &p.path);
            }
        }
        methods::FILE_SAVED => {
            if let Some(p) = parse::<FileParams>(params) {
                plugin.file_saved_from(host, &p.path, p.previous.as_deref());
            }
        }
        methods::FILE_CLOSED => {
            if let Some(p) = parse::<FileParams>(params) {
                plugin.file_closed(host, &p.path);
            }
        }
        methods::FILE_DELETED => {
            if let Some(p) = parse::<FileParams>(params) {
                plugin.file_deleted(host, &p.path, p.previous.as_deref());
                if !p.options.is_empty() {
                    plugin.delete_chosen(host, &p.path, &p.options);
                }
            }
        }
        methods::FILE_RENAMED => {
            if let Some(p) = parse::<RenameParams>(params) {
                plugin.file_renamed(host, &p.from, &p.to);
            }
        }
        methods::ACTIVE_FILE_CHANGED => {
            if let Some(p) = parse::<ActiveFileParams>(params) {
                plugin.active_file_changed(host, p.path.as_deref());
            }
        }
        methods::SETTINGS_CHANGED => {
            if let Some(settings) = parse::<Settings>(params) {
                plugin.settings_changed(host, &settings);
            }
        }
        _ => {}
    }
}

/// Reads the input on a thread: the answers to the plugin's requests go to
/// whoever waits for them (the plugin may be waiting on the main thread),
/// the rest comes out of the receiver.
fn read_in_background(
    mut input: impl io::BufRead + Send + 'static,
    pending: Pending,
) -> Receiver<Message> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        while let Ok(Some(message)) = read_message(&mut input) {
            if let Message::Response { id, result } = message {
                let waiter = pending.lock().ok().and_then(|mut p| p.remove(&id));
                if let Some(waiter) = waiter {
                    let _ = waiter.send(result);
                }
            } else if tx.send(message).is_err() {
                break;
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_for_tests_writes_its_notifications_where_it_is_told() {
        #[derive(Clone, Default)]
        struct Shared(Arc<Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().write(buf)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let out = Shared::default();
        let host = Host::to_writer(out.clone());
        host.set_origin("/tmp/a", Some("box:/a".into()));
        let bytes = out.0.lock().unwrap().clone();
        let said = crate::protocol::read_message(&mut io::BufReader::new(&bytes[..]))
            .unwrap()
            .unwrap();
        match said {
            Message::Notification { method, params } => {
                assert_eq!(method, methods::SET_ORIGIN);
                assert_eq!(params["origin"], "box:/a");
            }
            other => panic!("{other:?}"),
        }
    }
    use crate::ui::text;
    use serde_json::json;
    use std::path::PathBuf;

    /// A buffer the test can read after the plugin wrote to it.
    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn framed(messages: &[Message]) -> Vec<u8> {
        let mut out = Vec::new();
        for m in messages {
            write_message(&mut out, m).unwrap();
        }
        out
    }

    fn read_all(bytes: &[u8]) -> Vec<Message> {
        let mut input = BufReader::new(bytes);
        std::iter::from_fn(|| read_message(&mut input).unwrap()).collect()
    }

    #[derive(Default)]
    struct Recorder {
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl Plugin for Recorder {
        fn initialize(&mut self, _: &Host, params: &InitializeParams) {
            self.seen
                .lock()
                .unwrap()
                .push(format!("init {}", params.settings.locale));
        }
        fn command(&mut self, host: &Host, command: &str, _: &Context) {
            self.seen.lock().unwrap().push(format!("command {command}"));
            host.notify("done");
        }
        fn panel_opened(&mut self, host: &Host, panel: &str) {
            host.render(panel, text("hi"));
        }
        fn ui_event(&mut self, _: &Host, event: &UiEvent) {
            self.seen
                .lock()
                .unwrap()
                .push(format!("{} {:?}", event.action, event.text()));
        }
        fn shutdown(&mut self, _: &Host) {
            self.seen.lock().unwrap().push("shutdown".into());
        }
    }

    #[test]
    fn the_plugin_hears_the_editor_and_answers_it() {
        let init = InitializeParams {
            api: 1,
            app_version: "1.28.0".into(),
            settings: Settings {
                locale: "fr".into(),
                dark: true,
            },
            workspace: vec![],
            plugin_dir: PathBuf::from("/p"),
            data_dir: PathBuf::from("/d"),
        };
        let input = framed(&[
            Message::request(1, methods::INITIALIZE, &init),
            Message::notification(
                methods::COMMAND,
                json!({"command": "hello.greet", "context": {}}),
            ),
            Message::notification(methods::PANEL_OPENED, json!({"panel": "main"})),
            Message::notification(
                methods::UI_EVENT,
                json!({"panel": "main", "action": "name", "value": "Ada"}),
            ),
            Message::request(2, methods::SHUTDOWN, Value::Null),
        ]);
        let out = Shared::default();
        let plugin = Recorder::default();
        let seen = plugin.seen.clone();
        run_on(
            BufReader::new(io::Cursor::new(input)),
            Box::new(out.clone()),
            plugin,
        )
        .unwrap();

        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                "init fr",
                "command hello.greet",
                "name Some(\"Ada\")",
                "shutdown"
            ]
        );
        let written = read_all(&out.0.lock().unwrap());
        assert_eq!(
            written,
            vec![
                Message::Response {
                    id: 1,
                    result: Ok(Value::Null)
                },
                Message::notification(methods::NOTIFY, json!({"message": "done", "level": "info"})),
                Message::notification(
                    methods::RENDER,
                    json!({"panel": "main", "root": {"type": "text", "text": "hi"}})
                ),
                Message::Response {
                    id: 2,
                    result: Ok(Value::Null)
                },
            ]
        );
    }

    struct Linter;

    impl Plugin for Linter {
        fn lint(&mut self, _: &Host, params: &LintParams) -> Result<LintResult, String> {
            Ok(LintResult {
                ran: vec![format!("{} bytes", params.text.len())],
                language_server: true,
                ..LintResult::default()
            })
        }
    }

    #[test]
    fn requests_of_the_editor_are_answered_or_refused() {
        let input = framed(&[
            Message::request(
                1,
                methods::LINT_RUN,
                json!({"path": "/a.js", "text": "abc"}),
            ),
            Message::request(2, methods::LINT_FIX, json!({"path": "/a.js", "text": ""})),
            Message::request(3, "other/thing", Value::Null),
            Message::request(4, methods::LINT_RUN, json!({"text": 1})),
        ]);
        let out = Shared::default();
        run_on(
            BufReader::new(io::Cursor::new(input)),
            Box::new(out.clone()),
            Linter,
        )
        .unwrap();
        let written = read_all(&out.0.lock().unwrap());
        let Message::Response {
            id: 1,
            result: Ok(v),
        } = &written[0]
        else {
            panic!("{written:?}");
        };
        assert_eq!(v["ran"][0], "3 bytes");
        assert_eq!(v["language_server"], true);
        let code = |m: &Message| match m {
            Message::Response { result: Err(e), .. } => e.code,
            _ => 0,
        };
        assert_eq!(code(&written[1]), codes::INTERNAL_ERROR);
        assert_eq!(code(&written[2]), codes::METHOD_NOT_FOUND);
        assert_eq!(code(&written[3]), codes::INVALID_PARAMS);
    }

    #[test]
    fn a_plugin_adds_boxes_before_a_deletion_and_hears_the_ones_ticked() {
        type Chosen = Vec<(PathBuf, Vec<String>)>;
        #[derive(Default)]
        struct Tracker(Arc<Mutex<Chosen>>);
        impl Plugin for Tracker {
            fn delete_options(&mut self, _: &Host, path: &Path) -> Vec<DeleteOption> {
                vec![DeleteOption {
                    id: "untrack".into(),
                    label: format!("Untrack {}", path.display()),
                    checked: true,
                }]
            }
            fn delete_chosen(&mut self, _: &Host, path: &Path, options: &[String]) {
                self.0
                    .lock()
                    .unwrap()
                    .push((path.to_path_buf(), options.to_vec()));
            }
        }
        let input = framed(&[
            Message::request(1, methods::DELETE_OPTIONS, json!({"path": "/a.txt"})),
            Message::notification(
                methods::FILE_DELETED,
                json!({"path": "/a.txt", "options": ["untrack"]}),
            ),
            Message::notification(methods::FILE_DELETED, json!({"path": "/b.txt"})),
        ]);
        let out = Shared::default();
        let chosen = Arc::new(Mutex::new(Vec::new()));
        run_on(
            BufReader::new(io::Cursor::new(input)),
            Box::new(out.clone()),
            Tracker(chosen.clone()),
        )
        .unwrap();
        let written = read_all(&out.0.lock().unwrap());
        let Message::Response {
            id: 1,
            result: Ok(v),
        } = &written[0]
        else {
            panic!("{written:?}");
        };
        assert_eq!(v["options"][0]["id"], "untrack");
        assert_eq!(v["options"][0]["checked"], true);
        assert_eq!(
            *chosen.lock().unwrap(),
            vec![(PathBuf::from("/a.txt"), vec!["untrack".to_string()])],
            "only the deletion with a box ticked"
        );
    }

    #[test]
    fn a_plugin_forgets_its_data_when_asked() {
        struct Keeper(Arc<Mutex<bool>>);
        impl Plugin for Keeper {
            fn forget_data(&mut self, _: &Host) -> Result<(), String> {
                *self.0.lock().unwrap() = true;
                Ok(())
            }
        }
        struct Failing;
        impl Plugin for Failing {
            fn forget_data(&mut self, _: &Host) -> Result<(), String> {
                Err("the keychain is locked".into())
            }
        }
        let input = || framed(&[Message::request(1, methods::FORGET_DATA, Value::Null)]);
        let forgot = Arc::new(Mutex::new(false));
        let out = Shared::default();
        run_on(
            BufReader::new(io::Cursor::new(input())),
            Box::new(out.clone()),
            Keeper(forgot.clone()),
        )
        .unwrap();
        assert!(*forgot.lock().unwrap());
        let written = read_all(&out.0.lock().unwrap());
        assert!(matches!(
            &written[0],
            Message::Response {
                id: 1,
                result: Ok(Value::Null)
            }
        ));
        let out = Shared::default();
        run_on(
            BufReader::new(io::Cursor::new(input())),
            Box::new(out.clone()),
            Failing,
        )
        .unwrap();
        let written = read_all(&out.0.lock().unwrap());
        assert!(
            matches!(&written[0], Message::Response { result: Err(e), .. } if e.message.contains("locked"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_request_of_the_plugin_gets_the_editors_answer() {
        // The editor answers request 1 (the plugin's first) right away.
        let (reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        let out = Shared::default();
        let host = Host {
            out: Arc::new(Mutex::new(Box::new(out.clone()))),
            pending: Arc::default(),
            next_id: Arc::new(Mutex::new(0)),
        };
        let _rx = read_in_background(BufReader::new(reader), host.pending.clone());
        let answer = std::thread::spawn(move || host.confirm("Sure?"));
        // Wait for the request to be written before answering it.
        while out.0.lock().unwrap().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        write_message(
            &mut writer,
            &Message::Response {
                id: 1,
                result: Ok(json!(true)),
            },
        )
        .unwrap();
        assert_eq!(answer.join().unwrap(), Ok(true));
        assert_eq!(
            read_all(&out.0.lock().unwrap()),
            vec![Message::request(
                1,
                methods::CONFIRM,
                json!({"message": "Sure?"})
            )]
        );
    }
}
