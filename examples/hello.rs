//! The smallest plugin with a command and a panel:
//! `cargo run --example hello` waits for the editor on its input.

use ardoise_plugin::protocol::{Context, UiEvent};
use ardoise_plugin::ui::*;
use ardoise_plugin::{run, Host, Plugin};

#[derive(Default)]
struct Hello {
    name: String,
}

impl Hello {
    fn view(&self) -> Node {
        column([
            text("Hello").style(TextStyle::Heading),
            input("name", &self.name, "Your name").on_submit("greet"),
            button("Say hello", "greet").button_style(ButtonStyle::Primary),
        ])
        .spacing(8.0)
        .padding(12.0)
    }
}

impl Plugin for Hello {
    fn command(&mut self, host: &Host, command: &str, _: &Context) {
        if command == "hello.greet" {
            host.notify("Hello from a plugin!");
        }
    }

    fn panel_opened(&mut self, host: &Host, panel: &str) {
        host.render(panel, self.view());
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
