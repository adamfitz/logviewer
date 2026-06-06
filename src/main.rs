// Declare the keybinds module — tells the Rust compiler to look for
// src/keybinds.rs and compile it as part of this crate.
mod keybinds;

// Import the egui immediate-mode GUI library via eframe's re-export.
// eframe is the application framework (handles the window, event loop, OS integration).
// egui is the GUI library itself (widgets, layout, rendering).
use eframe::egui;

// Standard library imports for CLI argument parsing, file I/O, and path handling.
use std::env;
use std::fs;
use std::path::PathBuf;

// bring KeybindState into scope so we can use it without the module prefix.
use keybinds::KeybindState;

// The application state struct. In egui's immediate-mode model, this struct is
// the single source of truth — everything the UI needs to render a frame lives here.
// Each frame, egui calls ui() and re-draws the entire interface from this state.
struct LogViewerApp {
    // Log file stored as individual lines rather than one large String.
    // This is required for virtual/lazy row rendering via show_rows() —
    // egui needs a total row count upfront and must be able to index into
    // individual rows. Storing as Vec<String> gives us both of those for free.
    //
    // Performance rationale: with one big String, egui lays out every character
    // on every resize. With Vec<String> + show_rows(), only the ~30 visible lines
    // are laid out per frame regardless of file size, eliminating the maximise freeze.
    lines: Vec<String>,
    //
    // Owned instance of the keybind runtime state (enabled/disabled flag etc.).
    // Stored here so it persists across frames — egui re-calls ui() every frame
    // but the struct itself lives for the lifetime of the application.
    keybind_state: KeybindState,

    // The current search query entered by the user.
    // We render this as a visible text box so typed characters are shown.
    search_query: String,

    // When terminal search is requested with '/', focus the search field.
    search_focus_requested: bool,

    // The scroll offset to apply this frame, in pixels.
    // process_keybinds() writes into this each frame; the ScrollArea reads it.
    // Reset to 0.0 each frame after being consumed so movements don't accumulate.
    scroll_offset: f32,

    // Index into search_matches for the match to jump to on the next Enter press.
    current_match: usize,
}

impl LogViewerApp {
    // Associated constructor function (not a trait method).
    // Called once at startup to create the initial application state.
    // Takes a reference to a PathBuf rather than a String to correctly handle
    // cross-platform path representations (spaces, unicode, etc.).
    fn new(file_path: &PathBuf) -> Self {
        // Attempt to read the entire file into a String.
        // unwrap_or_else means: if reading fails (file not found, permissions, etc.),
        // instead of panicking we gracefully store an error message as the content.
        // This way the window still opens and displays a readable error to the user.
        let content = fs::read_to_string(file_path).unwrap_or_else(|err| {
            format!(
                "Failed to open log file: {}\nError: {}",
                file_path.display(), // .display() gives a human-readable path string
                err
            )
        });

        // Split the file content into individual owned lines.
        // .lines() is a standard iterator that splits on \n and \r\n (handles both
        // Unix and Windows line endings), and strips the newline characters themselves.
        // .map(|l| l.to_string()) converts each &str slice into an owned String
        // so the lines can outlive the temporary `content` String they came from.
        // .collect() gathers the iterator into our Vec<String>.
        let lines = content.lines().map(|l| l.to_string()).collect();

        // Rust's struct initialization shorthand works when the field name matches
        // the variable name. Here we also provide the remaining fields explicitly.
        Self {
            lines,
            keybind_state: KeybindState::new(),
            search_query: String::new(),
            search_focus_requested: false,
            scroll_offset: 0.0,
            current_match: 0,
        }
    }
}

// Implement the eframe::App trait for our state struct.
// This is the contract between our application logic and the eframe framework.
// eframe will call ui() on every frame (typically 60fps or on input events).
impl eframe::App for LogViewerApp {
    // ui() is the core render+update method introduced in eframe 0.34.
    // It replaces the older update(ctx, frame) signature.
    //
    // Parameters:
    //   ui    — a mutable reference to the current frame's UI context.
    //           This is what you use to add widgets (labels, buttons, scroll areas, etc.).
    //           In 0.34+, eframe provides this directly — no need to create a CentralPanel.
    //   frame — gives access to the eframe window itself (resize, close, set title, etc.).
    //           Prefixed with _ to suppress the "unused variable" compiler warning.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Choose the app-wide theme based on mode.
        // GUI mode uses a white background and black text.
        // Terminal mode uses a black background and white text.
        let mut visuals = if self.keybind_state.enabled {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        if self.keybind_state.enabled {
            visuals.extreme_bg_color = egui::Color32::BLACK;
            visuals.panel_fill = egui::Color32::BLACK;
            visuals.window_fill = egui::Color32::BLACK;
            visuals.faint_bg_color = egui::Color32::from_gray(20);
            visuals.override_text_color = Some(egui::Color32::WHITE);
        } else {
            visuals.extreme_bg_color = egui::Color32::WHITE;
            visuals.panel_fill = egui::Color32::WHITE;
            visuals.window_fill = egui::Color32::WHITE;
            visuals.faint_bg_color = egui::Color32::from_gray(240);
            visuals.override_text_color = Some(egui::Color32::BLACK);
        }

        ui.ctx().set_visuals(visuals.clone());
        ui.style_mut().visuals = visuals;

        // Render the mode-specific background for the entire app.
        // GUI mode uses an all-white background; terminal mode uses all-black.
        let frame_fill = if self.keybind_state.enabled {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        let frame = egui::Frame::new().fill(frame_fill);

        frame.show(ui, |ui| {
            // --- Header bar ---
            // A compact horizontal strip containing the search label, search input,
            // and mode toggle button. All elements share a consistent height so they
            // appear visually uniform regardless of mode.
            let search_response = ui.horizontal(|ui| {
                // Set a minimum row height so the label, text field, and button all
                // occupy the same vertical space and are aligned consistently.
                ui.set_min_height(32.0);

                // "Search:" label styled to visually match the mode button container
                // (same frame fill, corner radius, and inner margins) but without
                // button interactivity — it is a static label that signals the
                // search field.  The label takes its natural width so the remaining
                // header space can be split between the search box and the button.
                let btn_v = &ui.visuals().widgets.inactive;
                egui::Frame::default()
                    .fill(btn_v.weak_bg_fill)
                    .corner_radius(btn_v.corner_radius)
                    .stroke(btn_v.bg_stroke)
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        ui.set_min_height(32.0);
                        ui.label(egui::RichText::new("Search:").size(16.0));
                    });

                // Search box: use whatever space remains after the label and the
                // mode button (180 px) so the input always stretches to fill the
                // header without overflowing.
                let button_width = 180.0;
                let gap = ui.spacing().item_spacing.x;
                let search_width = (ui.available_width() - button_width - gap * 2.0).max(100.0);
                let response = ui.add_sized(
                    egui::vec2(search_width, 32.0),
                    egui::TextEdit::singleline(&mut self.search_query)
                        .font(egui::FontId::monospace(18.0))
                        .hint_text(
                            egui::RichText::new("type search and press Enter")
                                .font(egui::FontId::monospace(18.0)),
                        ),
                );

                // Mode toggle button positioned at the right edge via a
                // right-to-left sub-layout so it always sits consistently
                // at the far right of the header bar.
                let label = if self.keybind_state.enabled {
                    "Mode: Terminal (vim/less)"
                } else {
                    "Mode: GUI"
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_sized(
                            egui::vec2(button_width, 32.0),
                            egui::Button::new(egui::RichText::new(label).size(16.0)),
                        )
                        .clicked()
                    {
                        self.keybind_state.enabled = !self.keybind_state.enabled;
                    }
                });

                response
            });

            // Visual separator between the header bar and the log content below.
            ui.separator();

            let search_text_response = search_response.inner;
            let search_has_focus = search_text_response.has_focus();

            if self.search_focus_requested {
                search_text_response.request_focus();
                self.search_focus_requested = false;

                // Select all existing text
                if let Some(mut state) =
                    egui::TextEdit::load_state(ui.ctx(), search_text_response.id)
                {
                    let len = self.search_query.len();
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(len),
                        )));
                    state.store(ui.ctx(), search_text_response.id);
                }
            }

            // Measure the pixel height of a single monospace text row at the current
            // UI scale factor. This must match the text style used inside show_rows()
            // below — if they differ, row positions will be miscalculated and lines
            // will overlap or have gaps between them.
            // text_style_height() accounts for the font size AND the current display
            // scale (e.g. HiDPI/Retina), so it is always correct regardless of monitor.
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

            // Total number of rows in the file. show_rows() needs this to:
            //   1. correctly size the scrollbar thumb relative to total content
            //   2. calculate which row indices are visible at the current scroll position
            let filtered_lines: Vec<(usize, &String)> = if self.search_query.is_empty() {
                self.lines.iter().enumerate().collect()
            } else {
                self.lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.contains(&self.search_query))
                    .collect()
            };
            let total_rows = filtered_lines.len();

            // Build all match positions for Enter-based cycling through results.
            // Each entry is (filtered_line_index, byte_offset_of_match).
            let mut search_matches: Vec<(usize, usize)> = Vec::new();
            if !self.search_query.is_empty() {
                for (fi, (_, line)) in filtered_lines.iter().enumerate() {
                    let mut start = 0;
                    while let Some(pos) = line[start..].find(&self.search_query) {
                        let abs_pos = start + pos;
                        search_matches.push((fi, abs_pos));
                        start = abs_pos + self.search_query.len();
                    }
                }
            }
            // Clamp current_match whenever the match set changes.
            if self.current_match >= search_matches.len() {
                self.current_match = 0;
            }

            // Enter / Shift+Enter: cycle forward/backward through matches.
            // Works in both GUI and terminal modes when the search field is focused.
            // Enter advances to the next match; Shift+Enter goes back to the previous
            // one. The match we jump to becomes the highlighted current match
            // (the brighter background), so the user always sees which match is active.
            if search_has_focus && !search_matches.is_empty() {
                let (enter, shift) =
                    ui.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift));
                if enter && !shift {
                    // Advance to the next match (wrapping around to 0 at the end).
                    // We increment first, then scroll — this means the match we
                    // land on IS the highlighted one, not the next one in line.
                    self.current_match = (self.current_match + 1) % search_matches.len();
                    let (line_idx, _) = search_matches[self.current_match];
                    self.scroll_offset = line_idx as f32 * row_height;
                } else if enter && shift {
                    // Go back to the previous match (wrapping to the end at 0).
                    // Decrement first, then scroll — same logic as forward cycling
                    // but in reverse so the jumped-to match is highlighted.
                    self.current_match = if self.current_match == 0 {
                        search_matches.len() - 1
                    } else {
                        self.current_match - 1
                    };
                    let (line_idx, _) = search_matches[self.current_match];
                    self.scroll_offset = line_idx as f32 * row_height;
                }
            }

            // Process keybinds for this frame BEFORE rendering the ScrollArea.
            // This ensures scroll_offset is populated before the ScrollArea reads it,
            // so movements take effect on the same frame they are pressed (no 1-frame lag).
            //
            // process_keybinds() returns true if q was pressed and we should quit.
            let mut next_match = false;
            let mut prev_match = false;
            let should_quit = keybinds::process_keybinds(
                ui.ctx(),
                &mut self.keybind_state,
                &mut self.scroll_offset,
                total_rows,
                row_height,
                search_has_focus,
                &mut self.search_focus_requested,
                &mut next_match,
                &mut prev_match,
            );

            // Handle n/N from terminal-mode keybinds.
            // These flags are set by process_keybinds() above when the user presses
            // n (next match) or N / Shift+n (previous match) in terminal mode.
            // They only fire when search is NOT focused (so the letters aren't typed
            // into the search box). The logic mirrors Enter/Shift+Enter above:
            // we flip current_match first, then scroll, so the jumped-to match
            // gets the bright "current match" highlight.
            if next_match && !search_matches.is_empty() {
                self.current_match = (self.current_match + 1) % search_matches.len();
                let (line_idx, _) = search_matches[self.current_match];
                self.scroll_offset = line_idx as f32 * row_height;
            }
            if prev_match && !search_matches.is_empty() {
                self.current_match = if self.current_match == 0 {
                    search_matches.len() - 1
                } else {
                    self.current_match - 1
                };
                let (line_idx, _) = search_matches[self.current_match];
                self.scroll_offset = line_idx as f32 * row_height;
            }

            if should_quit {
                // eframe 0.34 does not expose a close() method on Frame, so we
                // exit immediately when q is pressed in terminal keybind mode.
                std::process::exit(0);
            }

            // Ctrl+F in GUI mode: focus the search bar (same as / in terminal mode).
            if !self.keybind_state.enabled
                && ui.input(|i| i.key_pressed(egui::Key::F) && i.modifiers.ctrl)
            {
                self.search_focus_requested = true;
            }

            // Ctrl+Down / Ctrl+Up in GUI mode: cycle forward/backward through matches
            // without needing the search field to be focused. This provides a keyboard
            // shortcut that matches common GUI convention (many editors/text fields use
            // Ctrl+Down/Ctrl+Up for similar navigation). The match we jump to is always
            // the one with the bright "current match" highlight.
            if !self.keybind_state.enabled && !search_matches.is_empty() {
                let (down, up) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowDown) && i.modifiers.ctrl,
                        i.key_pressed(egui::Key::ArrowUp) && i.modifiers.ctrl,
                    )
                });
                if down {
                    self.current_match = (self.current_match + 1) % search_matches.len();
                    let (line_idx, _) = search_matches[self.current_match];
                    self.scroll_offset = line_idx as f32 * row_height;
                }
                if up {
                    self.current_match = if self.current_match == 0 {
                        search_matches.len() - 1
                    } else {
                        self.current_match - 1
                    };
                    let (line_idx, _) = search_matches[self.current_match];
                    self.scroll_offset = line_idx as f32 * row_height;
                }
            }

            // Apply the scroll delta computed by process_keybinds() this frame.
            // vertical_scroll_offset() sets an absolute position; we add the delta
            // to whatever the current position is to get relative movement.
            // After consuming it, reset to 0.0 so the view does not keep scrolling
            // on frames where no key is pressed.
            let mut scroll_area = egui::ScrollArea::vertical().auto_shrink(false);
            if self.scroll_offset != 0.0 {
                // scroll_to_row would be cleaner for g/G but vertical_scroll_offset
                // is simpler and works correctly for all movements including page up/down.
                scroll_area = scroll_area.vertical_scroll_offset(self.scroll_offset);
                // Reset after consuming so movement stops when the key is released.
                self.scroll_offset = 0.0;
            }

            // Pre-compute text formats for search highlighting.
            // Three tiers of visual treatment:
            //   1. normal_fmt  — plain text, no match
            //   2. match_fmt   — a search match that is NOT the current one (subtle bg)
            //   3. current_fmt — the active/current match (bright bg + high-contrast fg)
            // The current match uses black text on a vivid gold background so it
            // "pops" visually against the regular match highlights and is easy to
            // spot even in large log files.
            let is_terminal = self.keybind_state.enabled;
            let monospace_font = ui
                .style()
                .text_styles
                .get(&egui::TextStyle::Monospace)
                .cloned()
                .unwrap_or_default();
            let (normal_color, match_bg, current_bg, current_fg) = if is_terminal {
                (
                    egui::Color32::WHITE,
                    egui::Color32::from_rgb(90, 85, 0),
                    egui::Color32::from_rgb(220, 200, 0),
                    egui::Color32::BLACK,
                )
            } else {
                (
                    egui::Color32::BLACK,
                    egui::Color32::from_rgb(255, 255, 180),
                    egui::Color32::from_rgb(255, 180, 0),
                    egui::Color32::BLACK,
                )
            };
            let normal_fmt = egui::text::TextFormat {
                font_id: monospace_font.clone(),
                color: normal_color,
                ..Default::default()
            };
            let match_fmt = egui::text::TextFormat {
                font_id: monospace_font.clone(),
                color: normal_color,
                background: match_bg,
                ..Default::default()
            };
            let current_fmt = egui::text::TextFormat {
                font_id: monospace_font,
                color: current_fg,
                background: current_bg,
                ..Default::default()
            };

            let query = &self.search_query;
            scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
                let range = row_range.clone();
                for (rel_fi, &(index, line)) in filtered_lines[range].iter().enumerate() {
                    let fi = row_range.start + rel_fi;
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            egui::vec2(72.0, row_height),
                            egui::Label::new(
                                egui::RichText::new(format!("{:>6}", index + 1)).monospace(),
                            ),
                        );
                        if query.is_empty() {
                            ui.add(egui::Label::new(egui::RichText::new(line).monospace()).wrap());
                        } else {
                            let mut job = egui::text::LayoutJob::default();
                            let mut start = 0;
                            while let Some(pos) = line[start..].find(query) {
                                let abs_pos = start + pos;
                                if abs_pos > start {
                                    job.append(&line[start..abs_pos], 0.0, normal_fmt.clone());
                                }
                                let is_current = search_matches
                                    .get(self.current_match)
                                    .map(|&(cm_fi, cm_off)| cm_fi == fi && cm_off == abs_pos)
                                    .unwrap_or(false);
                                let fmt = if is_current { &current_fmt } else { &match_fmt };
                                job.append(&line[abs_pos..abs_pos + query.len()], 0.0, fmt.clone());
                                start = abs_pos + query.len();
                            }
                            if start < line.len() {
                                job.append(&line[start..], 0.0, normal_fmt.clone());
                            }
                            ui.add(egui::Label::new(job).wrap());
                        }
                    });
                }
            });
        });
    }
}

// main() is the program entry point.
// It returns Result<(), eframe::Error> so that eframe startup failures
// (e.g. no display server, GPU init failure) propagate cleanly rather than panicking.
fn main() -> Result<(), eframe::Error> {
    // Collect all command-line arguments into a Vec<String>.
    // args[0] is always the binary name itself (e.g. "./logviewer").
    // args[1] onwards are the user-supplied arguments.
    let args: Vec<String> = env::args().collect();

    // Validate that the user provided at least one argument (the log file path).
    // args.len() < 2 means only args[0] (the binary name) exists — no file was given.
    if args.len() < 2 {
        // Print a usage hint to stderr (not stdout) — stderr is the correct stream
        // for error messages and diagnostics, stdout is for program output.
        eprintln!("Usage: {} <path/to/logfile.log>", args[0]);
        // Exit with code 1 to signal failure to the calling shell or parent process.
        // (Exit code 0 = success, anything else = failure by Unix convention.)
        std::process::exit(1);
    }

    // Convert the raw argument string into a PathBuf.
    // PathBuf is Rust's owned, mutable path type — it handles OS-specific separators
    // and is required by fs::read_to_string and other std::fs functions.
    let file_path = PathBuf::from(&args[1]);

    // Configure the native window options before creating it.
    // NativeOptions wraps everything eframe needs to initialise the OS window.
    let options = eframe::NativeOptions {
        // ViewportBuilder is a builder pattern for window properties.
        // with_inner_size sets the initial inner dimensions in logical pixels
        // (logical = physical pixels divided by the display scale factor,
        // so this looks the same size on both standard and HiDPI/Retina screens).
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),

        // ..Default::default() fills all other NativeOptions fields with their
        // defaults — vsync on, no fullscreen, system-native decorations, etc.
        ..Default::default()
    };

    // Start the eframe event loop. This call blocks until the window is closed.
    //
    // Arguments:
    //   "Log Viewer"  — the window title shown in the OS title bar / taskbar.
    //   options       — the window configuration built above.
    //   Box::new(...) — a heap-allocated closure that constructs our app state.
    //                   eframe calls this closure once during initialisation.
    //                   _cc is the CreationContext (fonts, render state, storage) — unused here.
    //                   Returns Ok(Box<dyn App>) as required by the eframe API.
    eframe::run_native(
        "Log Viewer",
        options,
        Box::new(|_cc| Ok(Box::new(LogViewerApp::new(&file_path)))),
    )
    // run_native returns Result<(), eframe::Error>, which we propagate directly
    // to main's return type via the implicit return (no semicolon).
}
