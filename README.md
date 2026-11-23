# ardoise-plugin

Write plugins for [Ardoise Editor](https://github.com/ardoise-editor/ardoise).

A plugin is a program with a `plugin.json` beside it. The editor starts it
and they talk [JSON-RPC 2.0](https://www.jsonrpc.org/specification) over its
standard input and output, framed as in the Language Server Protocol
(`Content-Length: <bytes>\r\n\r\n<json>`). The panels of a plugin are trees
of nodes (column, row, text, button, field, list, tree…) that the editor
draws with its own widgets, in its theme.

This crate holds the protocol, the manifest and the interface nodes, and a
runtime that does the talking for plugins written in Rust. A plugin can be
written in any language that reads and writes its standard streams: the
protocol is described in [the guide](https://ardoise-editor.github.io/ardoise/plugins.html).

## A plugin in Rust

```toml
[dependencies]
ardoise-plugin = { git = "https://github.com/ardoise-editor/plugin-sdk", tag = "v0.13.0" }
```

```rust
use ardoise_plugin::protocol::{Context, UiEvent};
use ardoise_plugin::ui::*;
use ardoise_plugin::{run, Host, Plugin};

#[derive(Default)]
struct Hello {
    name: String,
}

impl Plugin for Hello {
    fn command(&mut self, host: &Host, command: &str, _: &Context) {
        if command == "hello.greet" {
            host.notify("Hello from a plugin!");
        }
    }

    fn panel_opened(&mut self, host: &Host, panel: &str) {
        host.render(panel, column([
            text("Hello").style(TextStyle::Heading),
            input("name", &self.name, "Your name").on_submit("greet"),
            button("Say hello", "greet").button_style(ButtonStyle::Primary),
        ]));
    }

    fn ui_event(&mut self, host: &Host, event: &UiEvent) {
        match event.action.as_str() {
            "name" => self.name = event.text().unwrap_or_default().to_string(),
            "greet" => host.notify(format!("Hello, {}!", self.name)),
            _ => {}
        }
    }
}

fn main() -> std::io::Result<()> {
    run(Hello::default())
}
```

With its `plugin.json`:

```json
{
  "id": "acme.hello",
  "name": { "en": "Hello", "fr": "Bonjour" },
  "version": "0.1.0",
  "api": 1,
  "command": ["bin/hello"],
  "contributes": {
    "commands": [{ "id": "hello.greet", "title": "Say hello" }],
    "panels": [{ "id": "hello", "title": "Hello", "icon": "H" }]
  }
}
```

## Pictos and dialogs

A button can be a picto: `button("Refresh", "refresh").icon("refresh-cw")`
draws the [Lucide](https://lucide.dev/icons/) icon in a square button; the
label becomes its tooltip and its accessible name, `.tooltip("…")` says
something else. A button without `icon` is drawn as before.

`host.open_dialog(tab, title)` opens a window over the editor in place of a
tab (`host.open_tab`): the same frame everywhere, with the title, a cross and
Escape to close it. It shows what the plugin renders for `tab`; the plugin
hears `tab_closed` when it is closed and can close it with `host.close_tab`.

## A linter

A plugin with `"contributes": {"linter": {"extensions": ["py"]}}` is asked
`lint/run` for the problems of those files, and `lint/fix` for the text with
them fixed: implement `Plugin::lint` and `Plugin::fix` (types in
`ardoise_plugin::lint`). A panel with `"location": "settings"` is shown in
**Preferences › Plugins**, under the plugin.

[`plugin-template`](https://github.com/ardoise-editor/plugin-template) is a
repository to start from, with the workflow that publishes the releases the
editor installs.

## License

MIT
