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
    // The full text content of the log file, loaded once at startup.
    // Stored as a String so egui can display it as a text widget.
    // NOTE: This is a naive implementation — it loads the entire file into memory.
    // For large log files this will be replaced with memory-mapped byte streaming.
    log_content: String,
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
        let log_content = fs::read_to_string(file_path).unwrap_or_else(|err| {
            format!(
                "Failed to open log file: {}\nError: {}",
                file_path.display(), // .display() gives a human-readable path string
                err
            )
        });

        // Rust's struct initialisation shorthand — field name matches variable name,
        // so `log_content: log_content` can be written as just `log_content`.
        Self { log_content }
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

        // ScrollArea makes its contents scrollable when they exceed the available space.
        // ::vertical() means only vertical scrolling is enabled (no horizontal scroll).
        //
        // min_scrolled_height() sets the minimum height of the scrollable region.
        // ui.available_height() returns however much vertical space remains in the window
        // after the heading above — this makes the scroll area fill the rest of the window.
        egui::ScrollArea::vertical()
            .min_scrolled_height(ui.available_height())
            // .show() is where the ScrollArea actually renders.
            // It takes the parent ui and a closure; the closure receives a new child ui
            // that represents the interior of the scrollable area.
            .show(ui, |ui| {
                // Render the log content as monospaced (fixed-width) text.
                // Monospace is important for log files — it preserves alignment of
                // columns, stack traces, timestamps, and other structured output.
                // &self.log_content passes a string slice reference — no copy is made.
                ui.monospace(&self.log_content);
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
