//! The plugin side: implement [`Plugin`] and hand it to [`run`].
//!
//! ```no_run
//! use simpleedit_plugin::{run, Host, Plugin, ui::*};
//!
//! struct Hello;
//!
//! impl Plugin for Hello {
//!     fn command(&mut self, host: &Host, command: &str, _: &simpleedit_plugin::protocol::Context) {
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

use crate::protocol::{
    codes, methods, read_message, write_message, ActiveFile, CommandParams, ConfirmParams, Context,
    FileParams, InitializeParams, Level, Message, NotifyParams, OpenFileParams, PanelParams,
    RenderParams, RpcError, Settings, StatusParams, UiEvent,
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
    /// The user did something in a panel.
    fn ui_event(&mut self, host: &Host, event: &UiEvent) {}
    fn file_opened(&mut self, host: &Host, path: &Path) {}
    fn file_saved(&mut self, host: &Host, path: &Path) {}
    fn file_closed(&mut self, host: &Host, path: &Path) {}
    fn settings_changed(&mut self, host: &Host, settings: &Settings) {}
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
                    _ => Err(RpcError {
                        code: codes::METHOD_NOT_FOUND,
                        message: method.clone(),
                    }),
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
                plugin.file_saved(host, &p.path);
            }
        }
        methods::FILE_CLOSED => {
            if let Some(p) = parse::<FileParams>(params) {
                plugin.file_closed(host, &p.path);
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
