//! What a panel shows: a tree of nodes, sent whole with `ui/render`. The
//! editor draws it with its own widgets, in its theme; what the user does
//! comes back as a [`UiEvent`](crate::protocol::UiEvent).
//!
//! ```
//! use ardoise_plugin::ui::*;
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
        /// A key that presses it while its panel has the keyboard (the
        /// user clicked in it last): `"Ctrl+C"`, `"F4"`, `"Shift+Delete"`.
        /// `Ctrl` is Cmd on macOS.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        /// A square button with a picto instead of the label: the name of a
        /// Lucide icon (`"plus"`, `"refresh-cw"`, `"trash-2"`…) or one
        /// character, as for [`Item::icon`]. The label is then its tooltip
        /// (unless `tooltip` says otherwise) and its accessible name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        /// What a tooltip says when the pointer rests on the button.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
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
        /// Its width in pixels (none: all the room left in its row).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<f32>,
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
    /// A strip of tabs, the selected one underlined: a click sends the `id`
    /// of the strip and the `id` of the item.
    Tabs {
        id: String,
        items: Vec<Item>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<String>,
    },
    /// Tiles side by side (a picto above the label), one of them selected: a
    /// click sends the `id` of the tiles and the `id` of the item.
    Tiles {
        id: String,
        items: Vec<Item>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selected: Option<String>,
    },
    /// The footer of a dialog, set apart by a line: `start` at the left (the
    /// secondary actions, pictos), `end` at the right (Cancel, then the main
    /// action).
    Footer {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        start: Vec<Node>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        end: Vec<Node>,
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
        /// Ctrl+Space (and typing a name) asks the plugin what may come at
        /// the cursor (`completion/request`).
        #[serde(default, skip_serializing_if = "is_default")]
        completion: bool,
        /// Spans of lines (from 0, both ends in): the one the cursor is in
        /// is framed (else the next one, else the last), unless something
        /// is selected: the statement a Run would run.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        regions: Vec<[usize; 2]>,
        /// A word underlined in red (where an error is).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mark: Option<EditorMark>,
        /// Where the cursor goes, with the other end of a selection: taken
        /// when it differs from the one sent before (a match found).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<EditorSelection>,
    },
    /// Rows of cells under named columns (the rows of a table): only the
    /// rows on screen are drawn. What the user does comes back as
    /// `ui/event` with the `id` and a [`GridEvent`].
    Grid(Box<Grid>),
    /// A picture: SVG text.
    Image {
        svg: String,
        /// Its height in pixels (none: its own).
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
        /// Its height in pixels (none: all the room left).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<f32>,
    },
}

/// A grid of cells (see [`Node::Grid`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub id: String,
    pub columns: Vec<GridColumn>,
    pub rows: Vec<GridRow>,
    /// The cells picked: from `anchor` to `cursor` (row, column).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<GridSelection>,
    /// The cell being typed in, and what it holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editing: Option<GridEditing>,
    /// A field under each title to filter by that column.
    #[serde(default, skip_serializing_if = "is_default")]
    pub filters: bool,
    /// What a right click on a cell offers: each item sends `ui/event`
    /// with its `action` and `{"row", "col"}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<MenuItem>,
    /// Its height in pixels (none: all the room left).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GridColumn {
    pub name: String,
    /// Under the name, dimmed (a type).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// In pixels (none: as wide as it needs, within limits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    /// Sorted by it: ascending (`true`) or descending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<bool>,
    /// What its filter field holds.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub filter: String,
}

/// How a row stands against what is saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowState {
    #[default]
    Saved,
    New,
    Edited,
    Deleted,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GridRow {
    pub cells: Vec<GridCell>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub state: RowState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCell {
    pub text: String,
    /// No value (drawn as `NULL`).
    #[serde(default, skip_serializing_if = "is_default")]
    pub null: bool,
    /// Changed and not saved yet.
    #[serde(default, skip_serializing_if = "is_default")]
    pub edited: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSelection {
    pub anchor: (usize, usize),
    pub cursor: (usize, usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridEditing {
    pub row: usize,
    pub col: usize,
    pub value: String,
}

/// An item of a context menu.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    pub label: String,
    pub action: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// A key that does it on the cell of the cursor (`"F4"`, as the
    /// editor names keys), shown beside the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// What a grid sends back, as the `value` of its `ui/event`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GridEvent {
    /// Cells picked with the mouse or the arrows.
    Select {
        anchor: (usize, usize),
        cursor: (usize, usize),
    },
    /// A double click (or Enter, or F2) on a cell: to type in it.
    Open {
        row: usize,
        col: usize,
    },
    /// What the cell being typed in holds now.
    Edit {
        value: String,
    },
    /// Enter in the cell typed in; Escape.
    Commit,
    Cancel,
    /// A click on the title of a column.
    Sort {
        col: usize,
    },
    /// A column was made wider or narrower.
    Resize {
        col: usize,
        width: f32,
    },
    /// Its filter field changed (Enter: `submit` true).
    Filter {
        col: usize,
        value: String,
        submit: bool,
    },
    /// Scrolled to the last row (to load more).
    End,
    /// Ctrl+C on the cells picked; Ctrl+V with this text; Delete.
    Copy,
    Paste {
        text: String,
    },
    Delete,
}

impl GridEvent {
    /// What a grid sent, from the value of its event.
    pub fn from_event(event: &crate::protocol::UiEvent) -> Option<Self> {
        serde_json::from_value(event.value.clone()?).ok()
    }
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
    /// Its context menu: an item picked sends `ui/event` with the item's
    /// `action` and `{"item": id}`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<MenuItem>,
}

/// A place in an editor: a line and a column, in characters, from 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPos {
    pub line: usize,
    pub column: usize,
}

/// A word underlined in an editor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorMark {
    pub line: usize,
    pub column: usize,
    /// In characters.
    pub len: usize,
}

/// A cursor, and the other end of what it selects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorSelection {
    pub anchor: EditorPos,
    pub cursor: EditorPos,
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
        key: None,
        icon: None,
        tooltip: None,
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
        width: None,
    }
}

pub fn tabs(id: impl Into<String>, items: impl IntoIterator<Item = Item>) -> Node {
    Node::Tabs {
        id: id.into(),
        items: items.into_iter().collect(),
        selected: None,
    }
}

pub fn tiles(id: impl Into<String>, items: impl IntoIterator<Item = Item>) -> Node {
    Node::Tiles {
        id: id.into(),
        items: items.into_iter().collect(),
        selected: None,
    }
}

pub fn footer(start: impl IntoIterator<Item = Node>, end: impl IntoIterator<Item = Node>) -> Node {
    Node::Footer {
        start: start.into_iter().collect(),
        end: end.into_iter().collect(),
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
        menu: Vec::new(),
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
        completion: false,
        regions: Vec::new(),
        mark: None,
        selection: None,
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
        height: None,
    }
}

pub fn grid(
    id: impl Into<String>,
    columns: impl IntoIterator<Item = GridColumn>,
    rows: impl IntoIterator<Item = GridRow>,
) -> Grid {
    Grid {
        id: id.into(),
        columns: columns.into_iter().collect(),
        rows: rows.into_iter().collect(),
        ..Grid::default()
    }
}

/// A picture, from SVG text.
pub fn image(svg: impl Into<String>) -> Node {
    Node::Image {
        svg: svg.into(),
        height: None,
    }
}

impl From<Grid> for Node {
    fn from(grid: Grid) -> Node {
        Node::Grid(Box::new(grid))
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

    /// The height of an editor, a scroll or a picture, in pixels.
    pub fn height(mut self, value: f32) -> Self {
        match &mut self {
            Node::Editor { height, .. }
            | Node::Scroll { height, .. }
            | Node::Image { height, .. } => *height = Some(value),
            Node::Grid(grid) => grid.height = Some(value),
            _ => {}
        }
        self
    }

    /// An editor that asks the plugin what may come at the cursor.
    pub fn with_completion(mut self) -> Self {
        if let Node::Editor { completion, .. } = &mut self {
            *completion = true;
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

    /// A picto for a button (see [`Node::Button`]): its label becomes the
    /// tooltip and the accessible name.
    pub fn icon(mut self, value: impl Into<String>) -> Self {
        if let Node::Button { icon, .. } = &mut self {
            *icon = Some(value.into());
        }
        self
    }

    /// The tooltip of a button, in place of its label.
    pub fn tooltip(mut self, value: impl Into<String>) -> Self {
        if let Node::Button { tooltip, .. } = &mut self {
            *tooltip = Some(value.into());
        }
        self
    }

    /// The key that presses a button while its panel has the keyboard.
    pub fn key(mut self, value: impl Into<String>) -> Self {
        if let Node::Button { key, .. } = &mut self {
            *key = Some(value.into());
        }
        self
    }

    /// The spans of lines of an editor, the one of the cursor framed.
    pub fn regions(mut self, value: impl IntoIterator<Item = [usize; 2]>) -> Self {
        if let Node::Editor { regions, .. } = &mut self {
            *regions = value.into_iter().collect();
        }
        self
    }

    /// A word of an editor underlined in red.
    pub fn mark(mut self, value: EditorMark) -> Self {
        if let Node::Editor { mark, .. } = &mut self {
            *mark = Some(value);
        }
        self
    }

    /// Where the cursor of an editor goes (when it differs from before).
    pub fn selection(mut self, value: EditorSelection) -> Self {
        if let Node::Editor { selection, .. } = &mut self {
            *selection = Some(value);
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

    /// The row of a list (the tab, the tile) that is marked as selected.
    pub fn selected(mut self, value: impl Into<String>) -> Self {
        if let Node::List { selected, .. }
        | Node::Tabs { selected, .. }
        | Node::Tiles { selected, .. } = &mut self
        {
            *selected = Some(value.into());
        }
        self
    }

    /// The width of a field, in pixels.
    pub fn width(mut self, value: f32) -> Self {
        if let Node::Input { width, .. } = &mut self {
            *width = Some(value);
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

    /// Its context menu.
    pub fn menu(mut self, items: impl IntoIterator<Item = MenuItem>) -> Self {
        self.menu = items.into_iter().collect();
        self
    }
}

impl MenuItem {
    pub fn new(label: impl Into<String>, action: impl Into<String>) -> Self {
        MenuItem {
            label: label.into(),
            action: action.into(),
            enabled: true,
            key: None,
        }
    }

    pub fn enabled(mut self, value: bool) -> Self {
        self.enabled = value;
        self
    }

    /// The key that does it (see [`MenuItem::key`]).
    pub fn key(mut self, value: impl Into<String>) -> Self {
        self.key = Some(value.into());
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

#[cfg(test)]
mod grid_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_grid_goes_as_json_and_its_events_come_back() {
        let mut g = grid(
            "rows",
            [GridColumn {
                name: "id".into(),
                detail: "int".into(),
                ..GridColumn::default()
            }],
            [GridRow {
                cells: vec![GridCell {
                    text: "1".into(),
                    ..GridCell::default()
                }],
                state: RowState::Edited,
            }],
        );
        g.menu.push(MenuItem {
            label: "Copy".into(),
            action: "copy".into(),
            enabled: true,
            key: Some("F4".into()),
        });
        let node: Node = g.into();
        let node = node.height(200.0);
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["type"], "grid");
        assert_eq!(json["rows"][0]["state"], "edited");
        assert_eq!(json["height"], 200.0);
        assert!(json["menu"][0].get("enabled").is_none(), "true is left out");
        assert_eq!(json["menu"][0]["key"], "F4");
        let back: Node = serde_json::from_value(json).unwrap();
        assert_eq!(back, node);
        let event = crate::protocol::UiEvent {
            panel: "p".into(),
            action: "rows".into(),
            value: Some(serde_json::json!({"kind": "select", "anchor": [0, 1], "cursor": [2, 1]})),
        };
        assert_eq!(
            GridEvent::from_event(&event),
            Some(GridEvent::Select {
                anchor: (0, 1),
                cursor: (2, 1)
            })
        );
        // A scroll and a picture take a height too.
        assert_eq!(
            serde_json::to_value(scroll(text("x")).height(10.0)).unwrap()["height"],
            10.0
        );
        assert_eq!(
            serde_json::to_value(image("<svg/>")).unwrap()["type"],
            "image"
        );
    }

    #[test]
    fn a_button_with_a_picto_goes_as_json_and_back_and_without_one_is_as_before() {
        let plain = serde_json::to_value(button("Save", "save")).unwrap();
        assert!(plain.get("icon").is_none() && plain.get("tooltip").is_none());
        let old: Node =
            serde_json::from_str(r#"{"type":"button","label":"Save","action":"save"}"#).unwrap();
        assert_eq!(old, button("Save", "save"));
        let node = button("Refresh", "refresh")
            .icon("refresh-cw")
            .tooltip("Reload the tree");
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["icon"], "refresh-cw");
        assert_eq!(json["tooltip"], "Reload the tree");
        assert_eq!(serde_json::from_value::<Node>(json).unwrap(), node);
        // Only a button takes them.
        assert_eq!(text("x").icon("plus"), text("x"));
    }

    #[test]
    fn tabs_tiles_footers_and_the_width_of_a_field_go_as_json_and_back() {
        let node = column([
            tabs("tabs", [item("a", "A"), item("b", "B")]).selected("b"),
            tiles("kind", [item("pg", "PostgreSQL").icon("database")]).selected("pg"),
            input("port", "5432", "").width(110.0),
            footer(
                [button("Test", "test").icon("zap")],
                [button("Cancel", "cancel"), button("Save", "save")],
            ),
        ]);
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["children"][0]["type"], "tabs");
        assert_eq!(json["children"][0]["selected"], "b");
        assert_eq!(json["children"][1]["items"][0]["icon"], "database");
        assert_eq!(json["children"][2]["width"], 110.0);
        assert_eq!(json["children"][3]["end"][1]["action"], "save");
        assert_eq!(serde_json::from_value::<Node>(json).unwrap(), node);
        // A field without a width goes as before.
        assert!(serde_json::to_value(input("a", "", ""))
            .unwrap()
            .get("width")
            .is_none());
    }

    #[test]
    fn keys_menus_frames_marks_and_selections_go_as_json_and_back() {
        let node = column([
            button("Copy SQL", "copy").key("Ctrl+C"),
            tree(
                "tree",
                [tree_item("a", "A").menu([
                    MenuItem::new("Rename", "rename").key("F2"),
                    MenuItem::new("Delete", "delete").enabled(false),
                ])],
            ),
            editor("sql", "select 1;\nselect 2;", "sql")
                .regions([[0, 0], [1, 1]])
                .mark(EditorMark {
                    line: 1,
                    column: 7,
                    len: 1,
                })
                .selection(EditorSelection {
                    anchor: EditorPos { line: 0, column: 0 },
                    cursor: EditorPos { line: 0, column: 6 },
                }),
        ]);
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(json["children"][0]["key"], "Ctrl+C");
        let menu = &json["children"][1]["items"][0]["menu"];
        assert_eq!(menu[0]["key"], "F2");
        assert_eq!(menu[1]["enabled"], false);
        let editor = &json["children"][2];
        assert_eq!(editor["regions"], json!([[0, 0], [1, 1]]));
        assert_eq!(editor["mark"]["len"], 1);
        assert_eq!(editor["selection"]["cursor"]["column"], 6);
        let back: Node = serde_json::from_value(json).unwrap();
        assert_eq!(back, node);
        // Left out when not set.
        let plain = serde_json::to_value(editor_plain()).unwrap();
        for absent in ["regions", "mark", "selection"] {
            assert!(plain.get(absent).is_none(), "{absent}");
        }
        let plain = serde_json::to_value(button("b", "b")).unwrap();
        assert!(plain.get("key").is_none());
    }

    fn editor_plain() -> Node {
        editor("e", "", "")
    }
}
