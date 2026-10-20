//! Window rendering: header, the form, the preview and file panes, the
//! status bar, and the two dialogs.

mod highlight;
pub mod theme;

use egui::{
    Align, CursorIcon, Label, Layout, RichText, Sense, TextEdit, TextStyle, TextWrapMode, Ui,
};

use crate::command::{self, Answer, Command};
use crate::field::{self, Field, Kind, Value};
use crate::state::{Editing, Pending, State};
use egui::text::{CCursor, CCursorRange};

/// Gap between unrelated items.
const GAP: f32 = 10.0;

/// Formatted with the settings being edited, so that their effect shows.
const SAMPLE: &str = r#"// A level, as the game loads it.
Level(
    name: "Overgrown Ruins",
    size: (width: 64, height: 48),
    spawn_points: [(3, 4), (60, 44), (31, 2)],

    tiles: {"grass": 0, "stone": 1, "water": 2},
    // Enemies spawn in waves.
    waves: [
        Wave(count: 5, kind: Slime, delay_seconds: 2.5),
        Wave(count: 12, kind: Bat, delay_seconds: 1.0),
    ],
    music: Some("ruins_ambient.ogg"),
)
"#;

/// Panel frame without padding (the default panel frames add 8 px).
fn flat_panel(ui: &Ui) -> egui::Frame {
    egui::Frame::NONE.fill(ui.visuals().panel_fill)
}

pub fn render(state: &mut State, ui: &mut Ui) -> Option<Command> {
    let mut clicked = None;
    egui::Panel::top("header")
        .frame(flat_panel(ui))
        .show_separator_line(false)
        .show(ui, |ui| header(state, ui));
    clicked = clicked.or(egui::Panel::bottom("status")
        .frame(flat_panel(ui))
        .show(ui, status_bar)
        .inner);
    // Never narrower than the form's widest possible row.
    let width = form_width(ui);
    clicked = clicked.or(egui::Panel::left("form")
        .frame(flat_panel(ui).inner_margin(GAP))
        .resizable(true)
        .min_size(width)
        .default_size(width)
        .show_separator_line(false)
        .show(ui, |ui| form(state, ui))
        .inner);
    egui::CentralPanel::default()
        .frame(flat_panel(ui).inner_margin(GAP))
        .show(ui, |ui| {
            divider(ui);
            panes(state, ui);
        });
    if state.show_help {
        help(ui);
    }
    if let Some(pending) = &state.pending {
        clicked = clicked.or(confirm(pending, &state.doc.name, ui));
    }
    clicked
}

fn header(state: &State, ui: &mut Ui) {
    let p = &theme::P;
    egui::Frame::NONE
        .fill(p.surface)
        .inner_margin(egui::Margin::symmetric(GAP as i8, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&state.doc.name).color(p.strong).strong());
                if state.doc.is_dirty() {
                    ui.label(RichText::new("*").color(p.accent).strong());
                }
                if state.doc.is_new() {
                    ui.label(RichText::new("new file").color(p.weak));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("fmt.ron.ui").color(p.weak));
                    if let Some(notice) = &state.notice {
                        ui.add_space(GAP);
                        let color = if notice.error { p.red } else { p.green };
                        ui.label(RichText::new(&notice.text).color(color));
                    }
                });
            });
        });
}

/// The six fields. Clicking a row selects it; the widgets change values.
fn form(state: &mut State, ui: &mut Ui) -> Option<Command> {
    let p = &theme::P;
    let config = state.doc.config();
    let file = state.doc.parsed.as_ref().ok();
    let enabled = file.is_some();
    // Taken out for the loop; put back, committed or dropped after it.
    let mut editing = state.editing.take();
    let mut edit_done: Option<bool> = None;
    let mut out = None;
    let row_height = ui.spacing().interact_size.y + 4.0;
    let row_gap = 4.0;
    // The selected row's stripe, painted before the widgets so that it lies
    // under them. Every row has the minimum height: nothing in it is taller.
    let top = ui.cursor().top() + state.selected as f32 * (row_height + row_gap);
    // Up to the divider, which is painted over it.
    let x_range = egui::Rangef::new(ui.clip_rect().left(), ui.clip_rect().right() + 2.0);
    let y_range = egui::Rangef::new(top, top + row_height);
    ui.painter()
        .rect_filled(egui::Rect::from_x_y_ranges(x_range, y_range), 0.0, p.stripe);
    ui.painter()
        .vline(x_range.min + 1.0, y_range, egui::Stroke::new(2.0, p.accent));
    egui::Grid::new("fields")
        .num_columns(4)
        .spacing([GAP, row_gap])
        .min_row_height(row_height)
        .show(ui, |ui| {
            for (row, &field) in field::ALL.iter().enumerate() {
                let selected = row == state.selected;
                let set = file.and_then(|f| field.get(f));
                if checkbox(set.is_some(), enabled, ui).clicked() {
                    state.selected = row;
                    let value = (!set.is_some()).then(|| field.effective(&config));
                    out = Some(Command::Set(field, value));
                }
                let name = ui.add(
                    Label::new(RichText::new(field.name()).monospace().color(if selected {
                        p.strong
                    } else {
                        p.label
                    }))
                    .sense(Sense::click()),
                );
                if name.clicked() {
                    out = Some(Command::Select(row));
                }
                match set {
                    Some(_) if selected && editing.is_some() => {
                        let edit = editing.as_mut().expect("checked");
                        edit_done = edit_field(edit, ui);
                    }
                    Some(Value::Number(n)) => {
                        let value = ui.add(
                            Label::new(RichText::new(n.to_string()).monospace().color(p.strong))
                                .sense(Sense::click()),
                        );
                        if value.clicked() {
                            out = Some(Command::EditRow(row));
                        }
                    }
                    Some(Value::Choice(current)) => {
                        if let Some(new) = choice_widget(field, &current, ui) {
                            out = Some(Command::Set(field, Some(new)));
                        }
                    }
                    None => {
                        let inherited = ui.add(
                            Label::new(
                                RichText::new(format!("({})", field.effective(&config).literal()))
                                    .monospace()
                                    .color(p.weak),
                            )
                            .sense(Sense::click()),
                        );
                        if inherited.clicked() {
                            out = Some(Command::Set(field, Some(field.effective(&config))));
                        }
                    }
                }
                ui.label(RichText::new(field.flag()).monospace().color(p.weak));
                ui.end_row();
            }
        });
    match edit_done {
        None => state.editing = editing,
        Some(true) => {
            state.editing = editing;
            state.commit_edit();
        }
        Some(false) => {}
    }
    ui.add_space(GAP);
    ui.label(RichText::new(state.field().help()).color(p.weak).small());
    if let Some(problem) = state.doc.problem() {
        ui.add_space(GAP);
        ui.label(RichText::new(problem).color(p.red));
    }
    out
}

/// The text field of a value being typed. `Some(true)` when Enter commits
/// it, `Some(false)` when it is abandoned (Escape, or focus elsewhere).
fn edit_field(edit: &mut Editing, ui: &mut Ui) -> Option<bool> {
    let output = TextEdit::singleline(&mut edit.text)
        .font(TextStyle::Monospace)
        .desired_width(72.0)
        .show(ui);
    let response = output.response;
    if std::mem::take(&mut edit.fresh) {
        response.request_focus();
        let mut state = output.state;
        let all = CCursorRange::two(CCursor::new(0), CCursor::new(edit.text.chars().count()));
        state.cursor.set_char_range(Some(all));
        state.store(ui.ctx(), response.id);
        return None;
    }
    if response.lost_focus() {
        return Some(ui.input(|i| i.key_pressed(egui::Key::Enter)));
    }
    None
}

/// The drop-down of a choice field; `Some` when another choice is picked.
fn choice_widget(field: Field, current: &str, ui: &mut Ui) -> Option<Value> {
    let Kind::Choice(choices) = field.kind() else {
        return None;
    };
    let mut chosen = None;
    // Flush with the numbers in the column above and below.
    let padding = ui.spacing().button_padding;
    ui.spacing_mut().button_padding.x = 0.0;
    egui::ComboBox::from_id_salt(field.name())
        .selected_text(RichText::new(current).monospace().color(theme::P.strong))
        .show_ui(ui, |ui| {
            for &choice in choices {
                if ui
                    .selectable_label(choice == current, RichText::new(choice).monospace())
                    .clicked()
                {
                    chosen = Some(Value::Choice(choice.to_string()));
                }
            }
        });
    ui.spacing_mut().button_padding = padding;
    chosen
}

/// Side of a checkbox.
const CHECK: f32 = 16.0;

/// A square with a border, filled with the accent and ticked when checked.
fn checkbox(checked: bool, enabled: bool, ui: &mut Ui) -> egui::Response {
    let p = &theme::P;
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(CHECK, CHECK),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let rect = rect.shrink(1.0);
    let border = if response.hovered() && enabled {
        p.accent
    } else {
        p.border
    };
    let fill = if checked { p.accent } else { p.bg };
    ui.painter().rect(
        rect,
        0.0,
        fill,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    if checked {
        let tick = [
            egui::pos2(rect.left() + 3.0, rect.center().y),
            egui::pos2(rect.center().x - 1.0, rect.bottom() - 4.0),
            egui::pos2(rect.right() - 3.0, rect.top() + 4.0),
        ];
        ui.painter().add(egui::Shape::line(
            tick.to_vec(),
            egui::Stroke::new(2.0, p.on_accent),
        ));
    }
    if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// The width the form needs for its widest possible row: the checkbox, the
/// longest name, the widest value a field can show, and the longest flag.
fn form_width(ui: &Ui) -> f32 {
    let font = TextStyle::Monospace.resolve(ui.style());
    let column = ui.fonts_mut(|f| f.glyph_width(&font, ' '));
    let longest = |strings: &mut dyn Iterator<Item = &str>| {
        strings.map(str::len).max().unwrap_or(0) as f32 * column
    };
    let name = longest(&mut field::ALL.iter().map(|f| f.name()));
    let flag = longest(&mut field::ALL.iter().map(|f| f.flag()));
    let combo = ui.spacing().icon_width + ui.spacing().button_padding.x * 2.0 + GAP;
    let value = field::ALL
        .iter()
        .map(|f| match f.kind() {
            // "(1024)" inherited, or any number typed: allow eight digits.
            Kind::Number { .. } => 8.0 * column,
            Kind::Choice(choices) => longest(&mut choices.iter().copied()) * 1.0 + combo,
        })
        .fold(0.0, f32::max)
        .max(longest(&mut field::ALL.iter().map(|_| "(default)")));
    // Margins, the four columns, and the three gaps between them.
    2.0 * GAP + CHECK + name + value + flag + 3.0 * GAP
}

/// The line between the form and the panes, on whole pixels so it is crisp.
fn divider(ui: &Ui) {
    let p = &theme::P;
    let rect = ui.clip_rect();
    let x = ui.painter().round_to_pixel_center(rect.left());
    ui.painter()
        .vline(x, rect.y_range(), egui::Stroke::new(1.0, p.border));
}

/// A pane title with a note at the right.
fn section(ui: &mut Ui, title: &str, note: &str, note_color: egui::Color32) {
    let p = &theme::P;
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).color(p.label).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(note).color(note_color).small());
        });
    });
    ui.separator();
}

/// The sample formatted with the settings, and the file's text.
fn panes(state: &mut State, ui: &mut Ui) {
    let p = &theme::P;
    let config = state.doc.config();
    let half = (ui.available_height() - GAP) / 2.0;
    section(
        ui,
        "preview",
        &format!("sample at max_width {}", config.max_width),
        p.weak,
    );
    egui::ScrollArea::both()
        .id_salt("preview")
        .max_height(half - 2.0 * row_height(ui))
        .auto_shrink([false, false])
        .show(ui, |ui| preview(&config, ui));
    ui.add_space(GAP);
    let note = if state.doc.is_dirty() { "unsaved" } else { "" };
    section(ui, "file", note, p.accent);
    egui::ScrollArea::vertical()
        .id_salt("file")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let font = TextStyle::Monospace.resolve(ui.style());
            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                let job = highlight::layout(text.as_str(), font.clone(), wrap_width);
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let editor = ui.add(
                TextEdit::multiline(&mut state.doc.text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(6)
                    .frame(egui::Frame::NONE)
                    .layouter(&mut layouter),
            );
            if editor.changed() {
                state.doc.reparse();
            }
        });
}

fn row_height(ui: &Ui) -> f32 {
    ui.spacing().interact_size.y + ui.spacing().item_spacing.y
}

/// The formatted sample, with a ruler at the maximum width.
fn preview(config: &fmtron::Config, ui: &mut Ui) {
    let p = &theme::P;
    let text = match fmtron::format_ron(SAMPLE, config) {
        Ok(text) => text,
        Err(e) => e.to_string(),
    };
    let font = TextStyle::Monospace.resolve(ui.style());
    let job = highlight::layout(&text, font.clone(), f32::INFINITY);
    let label = ui.add(Label::new(job).wrap_mode(TextWrapMode::Extend));
    let column = ui.fonts_mut(|f| f.glyph_width(&font, ' '));
    let x = label.rect.left() + column * config.max_width as f32;
    ui.painter()
        .vline(x, label.rect.y_range(), egui::Stroke::new(1.0, p.border));
}

/// Clickable key hints.
fn status_bar(ui: &mut Ui) -> Option<Command> {
    let p = &theme::P;
    let mut clicked = None;
    egui::Frame::NONE
        .fill(p.surface)
        .inner_margin(egui::Margin::symmetric(GAP as i8, 4))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (keys, description, command) in command::HINTS {
                    let key = ui.add(
                        Label::new(
                            RichText::new(format!(" {keys} "))
                                .monospace()
                                .color(p.accent)
                                .background_color(p.hover),
                        )
                        .sense(Sense::click()),
                    );
                    let text = ui.add(
                        Label::new(RichText::new(*description).color(p.text)).sense(Sense::click()),
                    );
                    if key.hovered() || text.hovered() {
                        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                        ui.painter().hline(
                            text.rect.x_range(),
                            text.rect.bottom() - 1.0,
                            egui::Stroke::new(1.0, p.text),
                        );
                    }
                    if key.clicked() || text.clicked() {
                        clicked = Some(command.clone());
                    }
                    ui.add_space(GAP);
                }
            });
        });
    clicked
}

fn modal(id: &str, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) {
    let p = &theme::P;
    egui::Modal::new(egui::Id::new(id))
        .frame(
            egui::Frame::NONE
                .fill(p.surface)
                .stroke(egui::Stroke::new(1.0, p.accent))
                .inner_margin(GAP * 1.5),
        )
        .backdrop_color(egui::Color32::from_black_alpha(160))
        .show(ui.ctx(), |ui| {
            ui.set_max_width(ui.ctx().content_rect().width() * 0.8);
            add_contents(ui);
        });
}

fn help(ui: &mut Ui) {
    let p = &theme::P;
    modal("help", ui, |ui| {
        ui.label(RichText::new("Keys").color(p.label).strong());
        ui.add_space(4.0);
        egui::Grid::new("help_keys")
            .num_columns(2)
            .spacing([GAP * 2.0, 2.0])
            .show(ui, |ui| {
                for binding in command::BINDINGS.iter().filter(|b| !b.label.is_empty()) {
                    ui.label(RichText::new(binding.label).monospace().color(p.strong));
                    ui.label(binding.description);
                    ui.end_row();
                }
            });
        ui.add_space(GAP);
        ui.label(RichText::new("Editing").color(p.label).strong());
        ui.add_space(4.0);
        ui.label(
            "A checked field is written in the file; an unchecked one inherits \
             the default shown in parentheses. Changing a field rewrites only its \
             entry, then lays the file out as fmtron would, with the file's own \
             settings. Comments stay; a comment after an entry on the same line \
             goes with the entry when it is unset. The file pane can be edited \
             directly.",
        );
        ui.add_space(GAP);
        ui.label(RichText::new("Esc to close").color(p.weak).small());
    });
}

fn confirm(pending: &Pending, name: &str, ui: &mut Ui) -> Option<Command> {
    let p = &theme::P;
    let mut answer = None;
    modal("confirm", ui, |ui| {
        let then = match pending {
            Pending::Quit => "quitting",
            Pending::Open | Pending::Replace(_) => "opening another file",
        };
        ui.label(
            RichText::new(format!("{name} has unsaved changes."))
                .color(p.strong)
                .strong(),
        );
        ui.label(format!("Save them before {then}?"));
        ui.add_space(GAP);
        ui.horizontal(|ui| {
            for (label, key, choice) in [
                ("Save", "s", Answer::Save),
                ("Discard", "d", Answer::Discard),
                ("Cancel", "Esc", Answer::Cancel),
            ] {
                if ui.button(format!("{label} ({key})")).clicked() {
                    answer = Some(choice);
                }
            }
        });
    });
    answer.map(Command::Confirm)
}
