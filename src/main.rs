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
            // Render a large bold heading at the top of the window.
            // This consumes vertical space and moves the cursor down for subsequent widgets.
            // --- Top bar ---
            let search_response = ui.horizontal(|ui| {
                ui.heading("Log File Viewer");

                ui.add_space(16.0);
                ui.label("Search:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .desired_width(400.0)
                        .min_size(egui::vec2(400.0, 36.0))
                        .hint_text("type search and press Enter"),
                );

                // Spacer pushes the toggle button to the right side of the heading bar.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Toggle button — label changes to reflect current mode so the
                    // user always knows which mode they are in at a glance.
                    let label = if self.keybind_state.enabled {
                        "Mode: Terminal (vim/less)"
                    } else {
                        "Mode: GUI"
                    };
                    // ui.button() returns a Response; .clicked() is true for exactly
                    // one frame when the button is pressed, so this is a clean toggle.
                    if ui.button(label).clicked() {
                        self.keybind_state.enabled = !self.keybind_state.enabled;
                    }
                });

                response
            });

            let search_text_response = search_response.inner;
            let search_has_focus = search_text_response.has_focus();
            let search_lost_focus = search_text_response.lost_focus();

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

            if search_lost_focus && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                // User submitted the search query by pressing Enter.
                // If the search found matches, move focus away from the search box so
                // terminal navigation keys work again. If there are no matches, keep
                // the search box focused so the user can continue editing.
                if total_rows == 0 {
                    search_text_response.request_focus();
                } else {
                    search_text_response.surrender_focus();
                }
            }

            // Process keybinds for this frame BEFORE rendering the ScrollArea.
            // This ensures scroll_offset is populated before the ScrollArea reads it,
            // so movements take effect on the same frame they are pressed (no 1-frame lag).
            //
            // process_keybinds() returns true if q was pressed and we should quit.
            let should_quit = keybinds::process_keybinds(
                ui.ctx(),
                &mut self.keybind_state,
                &mut self.scroll_offset,
                total_rows,
                row_height,
                search_has_focus,
                &mut self.search_focus_requested,
            );

            if should_quit {
                // eframe 0.34 does not expose a close() method on Frame, so we
                // exit immediately when q is pressed in terminal keybind mode.
                std::process::exit(0);
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

            scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
                for &(index, line) in &filtered_lines[row_range] {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            egui::vec2(72.0, row_height),
                            egui::Label::new(
                                egui::RichText::new(format!("{:>6}", index + 1)).monospace(),
                            ),
                        );
                        ui.add(egui::Label::new(egui::RichText::new(line).monospace()).wrap());
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
