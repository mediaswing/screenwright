//! The egui/eframe desktop front-end for screenwright.
//!
//! Layout:
//! * a top menu/tool bar (New, Open, Save, Save As, view toggles);
//! * a left editor pane for raw Fountain source;
//! * a central preview pane with the formatted screenplay;
//! * a right statistics panel;
//! * a bottom status bar (file name, dirty flag, page count).
//!
//! Parsing is cached and only recomputed when the source text changes, so the
//! preview and stats stay cheap to display every frame.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui;

use crate::ai;
use crate::{format, parser, stats};

const STARTER: &str = "\
Title: Untitled
Credit: Written by
Author: Your Name

FADE IN:

INT. SOMEWHERE - DAY

Describe the scene here. Action lines are written flush left in the present tense.

A CHARACTER enters, looking around.

CHARACTER
(uncertain)
Is anyone there?

CUT TO:
";

/// Persistent application state.
pub struct ScreenwrightApp {
    /// Raw Fountain source shown in the editor.
    source: String,
    /// Path of the currently open file, if any.
    path: Option<PathBuf>,
    /// True when `source` differs from what is on disk.
    dirty: bool,
    /// Whether the right-hand statistics panel is visible.
    show_stats: bool,

    /// Cached formatted preview, regenerated when `source` changes.
    cache: Cache,
    /// Transient status message shown in the bottom bar.
    status: String,

    /// AI writing-prompt assistant state.
    ai: AiState,
}

/// State for the optional AI writing-prompt generator.
struct AiState {
    /// Whether the assistant window is open.
    open: bool,
    provider: ai::Provider,
    /// Model id (editable; seeded from the provider default).
    model: String,
    /// API key entered in the UI; falls back to the provider's env var.
    api_key: String,
    /// Optional topic/seed to steer the generated prompt.
    topic: String,
    /// The last generated prompt (or empty).
    result: String,
    /// True while a request is in flight.
    busy: bool,
    /// Receiver for the in-flight request's result.
    rx: Option<Receiver<Result<String, String>>>,
}

impl Default for AiState {
    fn default() -> Self {
        let provider = ai::Provider::Anthropic;
        AiState {
            open: false,
            provider,
            model: provider.default_model().to_string(),
            // Pre-fill from the environment if the user already exported a key.
            api_key: std::env::var(provider.env_var()).unwrap_or_default(),
            topic: String::new(),
            result: String::new(),
            busy: false,
            rx: None,
        }
    }
}

/// Memoized derived data so we only re-parse when the source actually changes.
struct Cache {
    /// Hash of the source the cache was built from.
    source_hash: u64,
    formatted: String,
    title: String,
    stats_report: String,
    page_estimate: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Cache {
            // A hash no real source will produce, forcing a first rebuild.
            source_hash: u64::MAX,
            formatted: String::new(),
            title: "Untitled".to_string(),
            stats_report: String::new(),
            page_estimate: 0,
        }
    }
}

impl ScreenwrightApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        ScreenwrightApp {
            source: STARTER.to_string(),
            path: None,
            dirty: false,
            show_stats: true,
            cache: Cache::default(),
            status: "Ready".to_string(),
            ai: AiState::default(),
        }
    }

    /// Rebuild the cached preview/stats if the source has changed.
    fn refresh_cache(&mut self) {
        let hash = fxhash(&self.source);
        if hash == self.cache.source_hash {
            return;
        }
        let sp = parser::parse(&self.source);
        let formatted = format::render(&sp);
        let stats = stats::compute(&sp, &formatted);
        let title = sp
            .meta("title")
            .map(|t| t.replace('\n', " "))
            .unwrap_or_else(|| self.file_stem());

        self.cache = Cache {
            source_hash: hash,
            stats_report: stats.report(&title),
            page_estimate: stats.estimated_pages,
            formatted,
            title,
        };
    }

    fn file_stem(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
            .to_string()
    }

    fn window_title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("untitled.fountain");
        let mark = if self.dirty { "• " } else { "" };
        format!("{mark}{name} — screenwright")
    }

    // ---- File operations -------------------------------------------------

    fn new_file(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        self.source = STARTER.to_string();
        self.path = None;
        self.dirty = false;
        self.status = "New screenplay".to_string();
    }

    fn open_file(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Fountain screenplay", &["fountain", "txt"])
            .pick_file()
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    self.source = text;
                    self.status = format!("Opened {}", path.display());
                    self.path = Some(path);
                    self.dirty = false;
                }
                Err(e) => self.status = format!("Open failed: {e}"),
            }
        }
    }

    /// Save to the current path, or prompt for one if there is none.
    fn save(&mut self) {
        match self.path.clone() {
            Some(path) => self.write_to(&path),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) {
        let default_name = format!("{}.fountain", sanitize(&self.cache.title));
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Fountain screenplay", &["fountain"])
            .set_file_name(default_name)
            .save_file()
        {
            self.write_to(&path);
            self.path = Some(path);
        }
    }

    /// Export the formatted preview to a plain-text `.txt` file.
    fn export_text(&mut self) {
        let default_name = format!("{}.txt", sanitize(&self.cache.title));
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Plain text", &["txt"])
            .set_file_name(default_name)
            .save_file()
        {
            match std::fs::write(&path, &self.cache.formatted) {
                Ok(()) => self.status = format!("Exported {}", path.display()),
                Err(e) => self.status = format!("Export failed: {e}"),
            }
        }
    }

    /// Export a print-ready PDF in 12pt Courier.
    fn export_pdf(&mut self) {
        let default_name = format!("{}.pdf", sanitize(&self.cache.title));
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(default_name)
            .save_file()
        {
            let bytes = crate::export::to_pdf(&self.cache.title, &self.cache.formatted);
            match std::fs::write(&path, bytes) {
                Ok(()) => self.status = format!("Exported {}", path.display()),
                Err(e) => self.status = format!("PDF export failed: {e}"),
            }
        }
    }

    /// Export an editable Word (.docx) document.
    fn export_docx(&mut self) {
        let default_name = format!("{}.docx", sanitize(&self.cache.title));
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Word document", &["docx"])
            .set_file_name(default_name)
            .save_file()
        {
            let sp = parser::parse(&self.source);
            match crate::export::write_docx(&sp, &path) {
                Ok(()) => self.status = format!("Exported {}", path.display()),
                Err(e) => self.status = format!("DOCX export failed: {e}"),
            }
        }
    }

    /// Render the AI writing-prompt assistant window.
    fn ai_window(&mut self, ctx: &egui::Context) {
        if !self.ai.open {
            return;
        }
        let mut open = self.ai.open;
        egui::Window::new("AI writing prompt")
            .open(&mut open)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.label(
                    "Brainstorm a screenplay prompt using your own Claude or ChatGPT account.",
                );
                ui.separator();

                // Provider — switching it reseeds the model and key defaults.
                ui.horizontal(|ui| {
                    ui.label("Provider:");
                    let before = self.ai.provider;
                    ui.radio_value(
                        &mut self.ai.provider,
                        ai::Provider::Anthropic,
                        ai::Provider::Anthropic.label(),
                    );
                    ui.radio_value(
                        &mut self.ai.provider,
                        ai::Provider::OpenAi,
                        ai::Provider::OpenAi.label(),
                    );
                    if self.ai.provider != before {
                        self.ai.model = self.ai.provider.default_model().to_string();
                        self.ai.api_key =
                            std::env::var(self.ai.provider.env_var()).unwrap_or_default();
                    }
                });

                egui::Grid::new("ai_fields")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Model:");
                        ui.text_edit_singleline(&mut self.ai.model);
                        ui.end_row();

                        ui.label("API key:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ai.api_key)
                                .password(true)
                                .hint_text(self.ai.provider.env_var()),
                        );
                        ui.end_row();

                        ui.label("Topic (optional):");
                        ui.text_edit_singleline(&mut self.ai.topic);
                        ui.end_row();
                    });

                ui.label(
                    egui::RichText::new(format!(
                        "Blank key falls back to ${}. Keys stay in memory only — never written to disk.",
                        self.ai.provider.env_var()
                    ))
                    .weak()
                    .small(),
                );

                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.ai.busy, egui::Button::new("Generate"))
                        .clicked()
                    {
                        self.start_ai_request(ui.ctx());
                    }
                    if self.ai.busy {
                        ui.spinner();
                        ui.label("Contacting the model…");
                    }
                });

                if !self.ai.result.is_empty() {
                    ui.separator();
                    // Read-only, selectable result (immutable &str → not editable).
                    let mut shown: &str = &self.ai.result;
                    ui.add(
                        egui::TextEdit::multiline(&mut shown).desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Insert into screenplay").clicked() {
                            self.insert_prompt();
                        }
                        if ui.button("Copy").clicked() {
                            ui.ctx().copy_text(self.ai.result.clone());
                        }
                    });
                }
            });
        self.ai.open = open;
    }

    /// Kick off an AI request on a background thread so the UI stays responsive.
    fn start_ai_request(&mut self, ctx: &egui::Context) {
        let config = ai::Config {
            provider: self.ai.provider,
            api_key: self.ai.api_key.clone(),
            model: self.ai.model.clone(),
        };
        let topic = self.ai.topic.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = ai::generate(&config, &topic);
            let _ = tx.send(result);
            // Wake the UI thread so it polls the channel promptly.
            ctx.request_repaint();
        });
        self.ai.rx = Some(rx);
        self.ai.busy = true;
        self.ai.result.clear();
        self.status = "Generating writing prompt…".to_string();
    }

    /// Check whether a background AI request has finished and apply its result.
    fn poll_ai_request(&mut self) {
        let Some(rx) = &self.ai.rx else { return };
        match rx.try_recv() {
            Ok(Ok(text)) => {
                self.ai.result = text;
                self.ai.busy = false;
                self.ai.rx = None;
                self.status = "Writing prompt ready".to_string();
            }
            Ok(Err(err)) => {
                self.ai.result = String::new();
                self.ai.busy = false;
                self.ai.rx = None;
                self.status = format!("AI error: {err}");
            }
            Err(TryRecvError::Empty) => {} // still working
            Err(TryRecvError::Disconnected) => {
                self.ai.busy = false;
                self.ai.rx = None;
            }
        }
    }

    /// Insert the generated prompt into the editor as a Fountain note.
    fn insert_prompt(&mut self) {
        if self.ai.result.is_empty() {
            return;
        }
        let note = format!("\n\n[[ Writing prompt: {} ]]\n", self.ai.result.trim());
        self.source.push_str(&note);
        self.dirty = true;
        self.status = "Prompt inserted as a note".to_string();
    }

    fn write_to(&mut self, path: &PathBuf) {
        match std::fs::write(path, &self.source) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved {}", path.display());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
    }

    /// Returns true if it is safe to discard the current buffer. A native
    /// dialog asks the user when there are unsaved changes.
    fn confirm_discard(&self) -> bool {
        if !self.dirty {
            return true;
        }
        let choice = rfd::MessageDialog::new()
            .set_title("Unsaved changes")
            .set_description("Discard unsaved changes to the current screenplay?")
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        matches!(choice, rfd::MessageDialogResult::Yes)
    }

    /// Handle Cmd/Ctrl keyboard shortcuts for the common file actions.
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
        let new = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::N));
        let open = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::O));
        let save = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::S));
        if new {
            self.new_file();
        }
        if open {
            self.open_file();
        }
        if save {
            self.save();
        }
    }
}

impl eframe::App for ScreenwrightApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);
        self.refresh_cache();
        self.poll_ai_request();

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // The assistant window floats above the panels; render it each frame.
        self.ai_window(&ctx);

        // ---- Top toolbar -------------------------------------------------
        // Every control here is a real egui button/checkbox, so each is
        // reachable with Tab / Shift+Tab and activatable with Enter or Space,
        // and is exposed to screen readers with its text as the accessible
        // name. Tooltips double as extra description for assistive tech.
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                if ui
                    .button("New")
                    .on_hover_text("Start a new screenplay (Cmd+N)")
                    .clicked()
                {
                    self.new_file();
                }
                if ui
                    .button("Open…")
                    .on_hover_text("Open a .fountain file (Cmd+O)")
                    .clicked()
                {
                    self.open_file();
                }
                if ui
                    .button("Save")
                    .on_hover_text("Save the screenplay (Cmd+S)")
                    .clicked()
                {
                    self.save();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as();
                }
                ui.separator();
                ui.menu_button("Export", |ui| {
                    if ui.button("PDF…").clicked() {
                        self.export_pdf();
                        ui.close();
                    }
                    if ui.button("Word document (.docx)…").clicked() {
                        self.export_docx();
                        ui.close();
                    }
                    if ui.button("Plain text…").clicked() {
                        self.export_text();
                        ui.close();
                    }
                })
                .response
                .on_hover_text("Export to PDF, Word, or plain text");
                ui.separator();
                if ui
                    .button("Writing prompt…")
                    .on_hover_text("Generate an AI writing prompt with your Claude or ChatGPT account")
                    .clicked()
                {
                    self.ai.open = true;
                }
                ui.separator();
                ui.checkbox(&mut self.show_stats, "Statistics")
                    .on_hover_text("Show or hide the statistics panel");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("Screenplay: {}", self.cache.title)).strong());
                });
            });
        });

        // ---- Bottom status bar ------------------------------------------
        // Plain-word labels (no symbol-only glyphs) so a screen reader speaks
        // them clearly, e.g. "Unsaved changes", "3 pages (estimated)".
        egui::Panel::bottom("status").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let saved = if self.dirty {
                    "Unsaved changes"
                } else {
                    "All changes saved"
                };
                ui.label(saved);
                ui.separator();
                ui.label(format!("{} pages (estimated)", self.cache.page_estimate));
                ui.separator();
                ui.label(format!("{} characters", self.source.len()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(&self.status);
                });
            });
        });

        // ---- Statistics side panel --------------------------------------
        if self.show_stats {
            egui::Panel::right("stats")
                .resizable(true)
                .default_size(260.0)
                .show_inside(ui, |ui| {
                    ui.add_space(4.0);
                    let label = ui.heading("Statistics");
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("stats_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            read_only_text(ui, &self.cache.stats_report, label.id);
                        });
                });
        }

        // ---- Editor (left) ----------------------------------------------
        egui::Panel::left("editor")
            .resizable(true)
            .default_size(480.0)
            .show_inside(ui, |ui| {
                ui.add_space(4.0);
                // The heading is the editor's visible label; we associate it
                // with the text field via `labelled_by` so a screen reader
                // announces "Screenplay source, edit text" on focus.
                let label = ui.heading("Screenplay source (Fountain)");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("editor_scroll")
                    .show(ui, |ui| {
                        // NOTE: deliberately *not* using `.code_editor()` here.
                        // That enables focus-lock, which makes Tab insert a
                        // tab character and traps keyboard users in the field.
                        // The default behaviour lets Tab move focus onward.
                        let response = ui
                            .add(
                                egui::TextEdit::multiline(&mut self.source)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30)
                                    .hint_text("Write your screenplay in Fountain format…"),
                            )
                            .labelled_by(label.id);
                        if response.changed() {
                            self.dirty = true;
                        }
                    });
            });

        // ---- Preview (center) -------------------------------------------
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.add_space(4.0);
            let label = ui.heading("Preview");
            ui.separator();
            egui::ScrollArea::both()
                .id_salt("preview_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    read_only_text(ui, &self.cache.formatted, label.id);
                });
        });
    }
}

/// Render `text` as a read-only, monospace, keyboard-focusable region.
///
/// This is egui's idiomatic read-only-text pattern: a multiline `TextEdit`
/// bound to an immutable `&str`. Because it is immutable, typing does nothing,
/// but it is still:
///   * **focusable** — reachable with Tab and showing a focus outline;
///   * **keyboard-scrollable** — arrow keys, PageUp/PageDown and Home/End move
///     the caret and the surrounding ScrollArea follows it into view;
///   * **selectable** — text can be selected and copied; and
///   * **screen-reader visible** — exposed as a text field via AccessKit.
///
/// `label_id` associates it with its section heading for assistive tech.
fn read_only_text(ui: &mut egui::Ui, text: &str, label_id: egui::Id) {
    // `TextEdit` needs `&mut`; a `&mut &str` is read-only to the widget.
    let mut shown = text;
    ui.add(
        egui::TextEdit::multiline(&mut shown)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    )
    .labelled_by(label_id);
}

/// A small, fast, non-cryptographic string hash (FNV-1a) used only to detect
/// when the editor contents change between frames.
fn fxhash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Turn a screenplay title into a safe default file name stem.
fn sanitize(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_lowercase()
    }
}
