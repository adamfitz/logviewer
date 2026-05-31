// Import the egui immediate-mode GUI library via eframe's re-export.
// eframe is the application framework (handles the window, event loop, OS integration).
// egui is the GUI library itself (widgets, layout, rendering).
use eframe::egui;

// Standard library imports for CLI argument parsing, file I/O, and path handling.
use std::env;
use std::fs;
use std::path::PathBuf;

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

        // Rust's struct initialisation shorthand — field name matches variable name,
        // so `lines: lines` can be written as just `lines`.
        Self { lines }
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
        // Render a large bold heading at the top of the window.
        // This consumes vertical space and moves the cursor down for subsequent widgets.
        ui.heading("Log Viewer");

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
        let total_rows = self.lines.len();

        // ScrollArea makes its contents scrollable when they exceed the available space.
        // ::vertical() means only vertical scrolling is enabled (no horizontal scroll).
        egui::ScrollArea::vertical()
            // Prevent the scroll area from shrinking smaller than the available space.
            // Without this, the scroll area can collapse when content is short,
            // leaving an unstyled gap at the bottom of the window.
            .auto_shrink(false)
            // show_rows() is the virtualised/lazy alternative to show().
            //
            // Instead of rendering all rows every frame, it:
            //   1. Calculates which row indices fall within the visible viewport
            //      based on the current scroll offset and row_height
            //   2. Inserts invisible spacer widgets above and below the visible range
            //      to maintain correct scroll position and scrollbar thumb size
            //   3. Only calls our closure for the visible rows (~20-40 at typical sizes)
            //
            // This means resizing or maximising the window only ever lays out the
            // rows currently visible — not the entire file — eliminating the freeze.
            //
            // Arguments:
            //   ui         — the parent UI context
            //   row_height — pixel height of each row (must match rendered text style)
            //   total_rows — total row count (for scrollbar and spacer calculations)
            //   closure    — called with (ui, row_range) where row_range is the
            //                Range<usize> of currently visible row indices
            .show_rows(ui, row_height, total_rows, |ui, row_range| {
                // row_range is a Range<usize> — e.g. 42..71 — representing only
                // the lines currently visible in the viewport. We slice our Vec
                // directly with this range; Rust's range slicing is zero-cost.
                for line in &self.lines[row_range] {
                    // Render each visible line as monospaced text.
                    // Monospace is important for log files — it preserves alignment
                    // of columns, stack traces, timestamps, and other structured output.
                    // Each call to monospace() renders exactly one line and advances
                    // the layout cursor down by row_height pixels.
                    ui.monospace(line);
                }
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
