//! What the keys and the clickable hints do.

use egui::{Context, Key, Modifiers};

use crate::field::{Field, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Open,
    Save,
    SaveAs,
    Quit,
    Up,
    Down,
    Select(usize),
    /// Set the selected field to its default, or unset it.
    Toggle,
    /// Step the selected field's number, or cycle its choice.
    Adjust(i64),
    /// Focus the selected field's value for typing.
    EditValue,
    /// A value clicked: select its row and type it.
    EditRow(usize),
    /// A value chosen with the mouse.
    Set(Field, Option<Value>),
    Help,
    Confirm(Answer),
}

/// The answer to "unsaved changes".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}

pub struct Binding {
    pub modifiers: Modifiers,
    pub key: Key,
    pub command: Command,
    /// How the key is written in the help.
    pub label: &'static str,
    pub description: &'static str,
}

const fn bind(
    modifiers: Modifiers,
    key: Key,
    command: Command,
    label: &'static str,
    description: &'static str,
) -> Binding {
    Binding {
        modifiers,
        key,
        command,
        label,
        description,
    }
}

/// Checked in order, so Ctrl+Shift+S comes before Ctrl+S.
pub const BINDINGS: &[Binding] = &[
    bind(
        Modifiers::COMMAND.plus(Modifiers::SHIFT),
        Key::S,
        Command::SaveAs,
        "Ctrl+Shift+S",
        "save as",
    ),
    bind(Modifiers::COMMAND, Key::S, Command::Save, "Ctrl+S", "save"),
    bind(Modifiers::COMMAND, Key::O, Command::Open, "Ctrl+O", "open"),
    bind(Modifiers::COMMAND, Key::Q, Command::Quit, "Ctrl+Q", "quit"),
    bind(
        Modifiers::NONE,
        Key::J,
        Command::Down,
        "j / ↓",
        "next field",
    ),
    bind(Modifiers::NONE, Key::ArrowDown, Command::Down, "", ""),
    bind(
        Modifiers::NONE,
        Key::K,
        Command::Up,
        "k / ↑",
        "previous field",
    ),
    bind(Modifiers::NONE, Key::ArrowUp, Command::Up, "", ""),
    bind(
        Modifiers::NONE,
        Key::Space,
        Command::Toggle,
        "Space",
        "set the field, or unset it to inherit the default",
    ),
    // Shifted before plain: a plain pattern also matches a shifted press.
    bind(
        Modifiers::SHIFT,
        Key::ArrowRight,
        Command::Adjust(10),
        "Shift+← / →",
        "step by 10",
    ),
    bind(
        Modifiers::SHIFT,
        Key::ArrowLeft,
        Command::Adjust(-10),
        "",
        "",
    ),
    bind(
        Modifiers::NONE,
        Key::ArrowRight,
        Command::Adjust(1),
        "← / →",
        "step the value",
    ),
    bind(Modifiers::NONE, Key::ArrowLeft, Command::Adjust(-1), "", ""),
    bind(
        Modifiers::NONE,
        Key::Enter,
        Command::EditValue,
        "Enter",
        "type the value",
    ),
    bind(Modifiers::NONE, Key::H, Command::Help, "h / F1", "help"),
    bind(Modifiers::NONE, Key::F1, Command::Help, "", ""),
];

/// The status bar's key hints.
pub const HINTS: &[(&str, &str, Command)] = &[
    ("^O", "open", Command::Open),
    ("^S", "save", Command::Save),
    ("^⇧S", "save as", Command::SaveAs),
    ("j/k", "move", Command::Down),
    ("Spc", "set/unset", Command::Toggle),
    ("←/→", "adjust", Command::Adjust(1)),
    ("Ret", "edit", Command::EditValue),
    ("h", "help", Command::Help),
    ("^Q", "quit", Command::Quit),
];

/// The commands of this frame's key presses, removed from the input so that
/// no widget acts on them too. Plain keys are left alone while a text field
/// has focus: they are typing.
pub fn take(ctx: &Context, text_focused: bool) -> Vec<Command> {
    let mut commands = Vec::new();
    for binding in BINDINGS {
        let plain = !binding.modifiers.command && !binding.modifiers.ctrl;
        if plain && text_focused {
            continue;
        }
        if ctx.input_mut(|i| i.consume_key(binding.modifiers, binding.key)) {
            commands.push(binding.command.clone());
        }
    }
    commands
}

/// Whether the focused widget is a text field (or a value being typed).
pub fn text_focused(ctx: &Context) -> bool {
    ctx.memory(|m| m.focused())
        .is_some_and(|id| egui::text_edit::TextEditState::load(ctx, id).is_some())
}
