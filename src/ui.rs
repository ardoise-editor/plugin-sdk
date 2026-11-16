//! What a panel shows: a tree of nodes, sent whole with `ui/render`. The
//! editor draws it with its own widgets, in its theme; what the user does
//! comes back as a [`UiEvent`](crate::protocol::UiEvent).
//!
//! ```
//! use simpleedit_plugin::ui::*;
//! let view = column([
//!     text("Hello").style(TextStyle::Heading),
//!     input("name", "", "Your name").on_submit("greet"),
//!     button("Say hello", "greet"),
//! ]);
//! ```

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    /// Nodes one under the other.
    Column {
        children: Vec<Node>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        padding: Option<f32>,
    },
    /// Nodes side by side.
    Row {
        children: Vec<Node>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spacing: Option<f32>,
    },
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "is_default")]
        style: TextStyle,
    },
    /// Sends `ui/event` with its `action` when pressed.
    Button {
        label: String,
        action: String,
        #[serde(default, skip_serializing_if = "is_default")]
        style: ButtonStyle,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        enabled: bool,
    },
    /// A line of text. Each change sends `ui/event` with the `id` and the
    /// text; Enter sends `submit` (if set) with the text.
    Input {
        id: String,
        value: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        placeholder: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        submit: Option<String>,
        /// Hides what is typed (passwords).
        #[serde(default, skip_serializing_if = "is_default")]
        secret: bool,
    },
    Checkbox {
        id: String,
        label: String,
        checked: bool,
    },
    /// A choice among a few options.
    Select {
        id: String,
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        placeholder: String,
    },
    /// Rows to click; a click sends the `id` of the list and of the row.
    List {
        id: String,
        items: Vec<Item>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<String>,
    },
    /// Rows that open on their children. A click on a row sends
    /// `{"item", "kind": "click"}`, its chevron `{"item", "kind": "toggle"}`:
    /// the plugin decides what opens (it may load the children then).
    Tree { id: String, items: Vec<TreeItem> },
    /// A thin line across.
    Separator,
    /// Empty room.
    Space { height: f32 },
    /// Text on several lines, highlighted as `language` (an extension:
    /// `json`, `rs`; empty: plain). Each change sends `ui/event` with the
    /// `id` and the whole text, unless it is `read_only`. What is typed stays
    /// until the plugin sends a different `value`.
    Editor {
        id: String,
        value: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        language: String,
        #[serde(default, skip_serializing_if = "is_default")]
        read_only: bool,
        /// Its height in pixels (none: all the room left).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f32>,
    },
    /// A terminal screen: a grid of styled cells the editor draws (see
    /// [`Screen`]). What the user types, the size it gets, the wheel and a
    /// paste come back as `ui/event` with the `id` (see [`ScreenInput`]).
    Screen(Box<Screen>),
    /// Its child scrolls when it is taller than the panel. With `bottom`, it
    /// stays at the end as the child grows (an output).
    Scroll {
        child: Box<Node>,
        #[serde(default, skip_serializing_if = "is_default")]
        bottom: bool,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    #[default]
    Normal,
    /// Grey, smaller: hints and details.
    Muted,
    Error,
    Bold,
    Heading,
    /// Monospaced: code, paths.
    Mono,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonStyle {
    Primary,
    #[default]
    Outline,
    /// No border: in rows of tools.
    Ghost,
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub label: String,
    /// Grey text after the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// One character drawn before the label: a glyph of the Nerd Fonts
    /// (<https://www.nerdfonts.com/cheat-sheet>) or any symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeItem {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeItem>,
    /// Shows a chevron even without children yet (they load when opened).
    #[serde(default, skip_serializing_if = "is_default")]
    pub expandable: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub expanded: bool,
}

/// What a terminal shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    pub id: String,
    /// The size of the grid the lines are for: (rows, columns).
    pub size: (u16, u16),
    /// The visible lines, top first.
    pub lines: Vec<Vec<Run>>,
    /// Where the cursor is drawn (none: hidden).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<(u16, u16)>,
    /// The arrows send `ESC O` (application cursor keys).
    #[serde(default, skip_serializing_if = "is_default")]
    pub app_cursor: bool,
    /// A paste is wrapped in `ESC [200~` … `ESC [201~`.
    #[serde(default, skip_serializing_if = "is_default")]
    pub bracketed_paste: bool,
    /// What a program asked of the mouse.
    #[serde(default, skip_serializing_if = "is_default")]
    pub mouse: MouseMode,
    /// The mouse is reported in the SGR encoding (else the old one).
    #[serde(default, skip_serializing_if = "is_default")]
    pub mouse_sgr: bool,
    /// A full-screen program (less, vim): the wheel moves its cursor.
    #[serde(default, skip_serializing_if = "is_default")]
    pub alternate: bool,
    /// The folder relative paths of the output are from (a click on
    /// `src/a.rs:3` opens it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<std::path::PathBuf>,
    /// A text marked wherever it is on screen (a search).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub highlight: String,
}

/// Cells of a line that look the same, from column `col`. A wide glyph
/// (two columns) is a run of its own, with `wide`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub col: u16,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<TermColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<TermColor>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub dim: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub inverse: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub wide: bool,
}

/// A color of a terminal: one of its 256 (the first 16 follow the theme of
/// the editor), or an exact one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermColor {
    Idx(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseMode {
    #[default]
    None,
    Press,
    PressRelease,
    ButtonMotion,
    AnyMotion,
}

/// What a screen sends back, as the `value` of its `ui/event`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScreenInput {
    /// Bytes to write to the program: keys, a paste, the mouse.
    Input { bytes: Vec<u8> },
    /// The screen has room for this grid.
    Resize { rows: u16, cols: u16 },
    /// The wheel over the history: lines up (positive) or down.
    Scroll { lines: i32 },
}

impl ScreenInput {
    /// What a screen sent, from the value of its event.
    pub fn from_event(event: &crate::protocol::UiEvent) -> Option<Self> {
        serde_json::from_value(event.value.clone()?).ok()
    }
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn is_true(value: &bool) -> bool {
    *value
}

fn yes() -> bool {
    true
}

// ─── Builders ─────────────────────────────────────────────────────────────

pub fn column(children: impl IntoIterator<Item = Node>) -> Node {
    Node::Column {
        children: children.into_iter().collect(),
        spacing: None,
        padding: None,
    }
}

pub fn row(children: impl IntoIterator<Item = Node>) -> Node {
    Node::Row {
        children: children.into_iter().collect(),
        spacing: None,
    }
}

pub fn text(text: impl Into<String>) -> Node {
    Node::Text {
        text: text.into(),
        style: TextStyle::Normal,
    }
}

pub fn button(label: impl Into<String>, action: impl Into<String>) -> Node {
    Node::Button {
        label: label.into(),
        action: action.into(),
        style: ButtonStyle::Outline,
        enabled: true,
    }
}

pub fn input(
    id: impl Into<String>,
    value: impl Into<String>,
    placeholder: impl Into<String>,
) -> Node {
    Node::Input {
        id: id.into(),
        value: value.into(),
        placeholder: placeholder.into(),
        submit: None,
        secret: false,
    }
}

pub fn checkbox(id: impl Into<String>, label: impl Into<String>, checked: bool) -> Node {
    Node::Checkbox {
        id: id.into(),
        label: label.into(),
        checked,
    }
}

pub fn select(
    id: impl Into<String>,
    options: impl IntoIterator<Item = impl Into<String>>,
    selected: Option<String>,
) -> Node {
    Node::Select {
        id: id.into(),
        options: options.into_iter().map(Into::into).collect(),
        selected,
        placeholder: String::new(),
    }
}

pub fn list(id: impl Into<String>, items: impl IntoIterator<Item = Item>) -> Node {
    Node::List {
        id: id.into(),
        items: items.into_iter().collect(),
        selected: None,
    }
}

pub fn tree(id: impl Into<String>, items: impl IntoIterator<Item = TreeItem>) -> Node {
    Node::Tree {
        id: id.into(),
        items: items.into_iter().collect(),
    }
}

pub fn item(id: impl Into<String>, label: impl Into<String>) -> Item {
    Item {
        id: id.into(),
        label: label.into(),
        detail: None,
        icon: None,
    }
}

pub fn tree_item(id: impl Into<String>, label: impl Into<String>) -> TreeItem {
    TreeItem {
        id: id.into(),
        label: label.into(),
        detail: None,
        icon: None,
        children: Vec::new(),
        expandable: false,
        expanded: false,
    }
}

pub fn editor(
    id: impl Into<String>,
    value: impl Into<String>,
    language: impl Into<String>,
) -> Node {
    Node::Editor {
        id: id.into(),
        value: value.into(),
        language: language.into(),
        read_only: false,
        height: None,
    }
}

pub fn separator() -> Node {
    Node::Separator
}

pub fn space(height: f32) -> Node {
    Node::Space { height }
}

pub fn scroll(child: Node) -> Node {
    Node::Scroll {
        child: Box::new(child),
        bottom: false,
    }
}

impl Node {
    /// Room between the children of a column or a row.
    pub fn spacing(mut self, value: f32) -> Self {
        if let Node::Column { spacing, .. } | Node::Row { spacing, .. } = &mut self {
            *spacing = Some(value);
        }
        self
    }

    /// An editor that is only read.
    pub fn read_only(mut self) -> Self {
        if let Node::Editor { read_only, .. } = &mut self {
            *read_only = true;
        }
        self
    }

    /// The height of an editor, in pixels.
    pub fn height(mut self, value: f32) -> Self {
        if let Node::Editor { height, .. } = &mut self {
            *height = Some(value);
        }
        self
    }

    /// A scroll that stays at the end as its child grows.
    pub fn stick_to_bottom(mut self) -> Self {
        if let Node::Scroll { bottom, .. } = &mut self {
            *bottom = true;
        }
        self
    }

    /// Room around a column.
    pub fn padding(mut self, value: f32) -> Self {
        if let Node::Column { padding, .. } = &mut self {
            *padding = Some(value);
        }
        self
    }

    /// The look of a text.
    pub fn style(mut self, value: TextStyle) -> Self {
        if let Node::Text { style, .. } = &mut self {
            *style = value;
        }
        self
    }

    /// The look of a button.
    pub fn button_style(mut self, value: ButtonStyle) -> Self {
        if let Node::Button { style, .. } = &mut self {
            *style = value;
        }
        self
    }

    /// A button that does nothing when `false`.
    pub fn enabled(mut self, value: bool) -> Self {
        if let Node::Button { enabled, .. } = &mut self {
            *enabled = value;
        }
        self
    }

    /// What Enter sends in a field.
    pub fn on_submit(mut self, action: impl Into<String>) -> Self {
        if let Node::Input { submit, .. } = &mut self {
            *submit = Some(action.into());
        }
        self
    }

    /// Hides what is typed in a field.
    pub fn secret(mut self) -> Self {
        if let Node::Input { secret, .. } = &mut self {
            *secret = true;
        }
        self
    }

    /// The text shown in an empty choice.
    pub fn placeholder(mut self, value: impl Into<String>) -> Self {
        if let Node::Select { placeholder, .. } = &mut self {
            *placeholder = value.into();
        }
        self
    }

    /// The row of a list that is marked as selected.
    pub fn selected(mut self, value: impl Into<String>) -> Self {
        if let Node::List { selected, .. } = &mut self {
            *selected = Some(value.into());
        }
        self
    }
}

impl Item {
    pub fn detail(mut self, value: impl Into<String>) -> Self {
        self.detail = Some(value.into());
        self
    }

    pub fn icon(mut self, value: impl Into<String>) -> Self {
        self.icon = Some(value.into());
        self
    }
}

impl TreeItem {
    pub fn detail(mut self, value: impl Into<String>) -> Self {
        self.detail = Some(value.into());
        self
    }

    pub fn icon(mut self, value: impl Into<String>) -> Self {
        self.icon = Some(value.into());
        self
    }

    pub fn children(mut self, value: impl IntoIterator<Item = TreeItem>) -> Self {
        self.children = value.into_iter().collect();
        self
    }

    /// Shows a chevron before the children are known.
    pub fn expandable(mut self) -> Self {
        self.expandable = true;
        self
    }

    pub fn expanded(mut self, value: bool) -> Self {
        self.expanded = value;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nodes_are_plain_json() {
        let view = column([
            text("Hi").style(TextStyle::Heading),
            button("Go", "go"),
            input("name", "", "Name").on_submit("go"),
        ])
        .spacing(8.0);
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            json!({
                "type": "column",
                "spacing": 8.0,
                "children": [
                    {"type": "text", "text": "Hi", "style": "heading"},
                    {"type": "button", "label": "Go", "action": "go"},
                    {"type": "input", "id": "name", "value": "", "placeholder": "Name", "submit": "go"},
                ]
            })
        );
    }

    #[test]
    fn a_screen_and_its_input_are_plain_json() {
        let node = Node::Screen(Box::new(Screen {
            id: "t".into(),
            size: (2, 4),
            lines: vec![vec![Run {
                col: 0,
                text: "ls".into(),
                fg: Some(TermColor::Idx(2)),
                ..Run::default()
            }]],
            cursor: Some((0, 2)),
            ..Screen::default()
        }));
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["type"], "screen");
        assert_eq!(json["lines"][0][0]["fg"], json!({"idx": 2}));
        assert_eq!(serde_json::from_value::<Node>(json).unwrap(), node);
        let input: ScreenInput =
            serde_json::from_value(json!({"kind": "resize", "rows": 3, "cols": 9})).unwrap();
        assert_eq!(input, ScreenInput::Resize { rows: 3, cols: 9 });
    }

    #[test]
    fn what_is_left_out_takes_its_default() {
        let node: Node =
            serde_json::from_value(json!({"type": "button", "label": "A", "action": "a"})).unwrap();
        assert_eq!(node, button("A", "a"));
        let node: Node = serde_json::from_value(json!({
            "type": "tree", "id": "t",
            "items": [{"id": "1", "label": "One", "children": [{"id": "2", "label": "Two"}]}]
        }))
        .unwrap();
        assert_eq!(
            node,
            tree(
                "t",
                [tree_item("1", "One").children([tree_item("2", "Two")])]
            )
        );
    }
}
