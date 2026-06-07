// keybinds.rs — vi/less-style keyboard navigation handler.
//
// This module is intentionally separate from main.rs to keep concerns isolated:
//   - main.rs owns the app state and rendering
//   - keybinds.rs owns all keyboard input logic
//
// To add or change a keybind, this is the only file that needs to be touched.

use eframe::egui;

// The scroll speed in pixels for single-line movements (j/k).
// This matches a typical terminal line height and feels natural for log navigation.
// Defined as a constant so it can be tuned in one place without hunting through logic.
const LINE_SCROLL_SPEED: f32 = 20.0;

// KeybindState holds the runtime state that the keybind system needs to track
// across frames. It is stored inside LogViewerApp in main.rs as a field.
//
// Keeping this in its own struct means main.rs does not need to know about
// individual keybind implementation details — it just holds one KeybindState
// and passes it to process_keybinds() each frame.
pub struct KeybindState {
    // Whether terminal keybind mode is currently active.
    // When false, the keyboard behaves normally (egui handles text input etc.).
    // When true, single keypresses are intercepted for vi/less-style navigation.
    pub enabled: bool,
}

impl KeybindState {
    // Constructor — keybind mode starts disabled so the app opens in normal GUI mode.
    // The user opts in via a button or menu item in main.rs.
    pub fn new() -> Self {
        Self { enabled: false }
    }
}

// process_keybinds() is the main entry point for this module.
// It is called once per frame from LogViewerApp::ui() in main.rs.
//
// Parameters:
//   ctx          — the egui context, used to read raw keyboard input and
//                  to access the scroll area's scroll delta this frame
//   state        — mutable reference to KeybindState so we can toggle mode on/off
//   scroll_delta — mutable f32 that this function writes the desired scroll
//                  amount into; main.rs applies this to the ScrollArea each frame
//   total_rows   — total number of lines in the file, used to calculate the
//                  maximum scroll position for G (jump to bottom)
//   row_height   — pixel height of one monospace row, used to convert line-based
//                  movements (j/k/f/b) into pixel scroll deltas
//   search_focused — true when the search text field currently has keyboard focus.
//                    In that case, we must not intercept keystrokes so the user can type.
//   next_match_requested — set to true when the user presses n (next match).
//                          main.rs reads this after the call and advances current_match.
//   prev_match_requested — set to true when the user presses N/shift+n (previous match).
//                          main.rs reads this after the call and rewinds current_match.
//   open_requested       — set to true when the user presses o (open file).
//                          main.rs reads this after the call and opens the file dialog.
//   follow_toggled       — set to true when the user presses t (toggle follow mode).
//                          main.rs reads this after the call and toggles tail -f.
//   reverse_search       — set to false on /, set to true on ?.
//                          main.rs uses this to swap match cycling direction.
//   go_to_top            — set to true when g is pressed (jump to first result).
//                          main.rs updates current_match accordingly.
//   go_to_bottom         — set to true when G is pressed (jump to last result).
//                          main.rs updates current_match accordingly.
//
// Returns true if the application should quit (q was pressed), false otherwise.
// main.rs checks this return value and calls frame.close() accordingly.
pub fn process_keybinds(
    ctx: &egui::Context,
    state: &mut KeybindState,
    scroll_delta: &mut f32,
    total_rows: usize,
    row_height: f32,
    search_focused: bool,
    search_focus_requested: &mut bool,
    next_match_requested: &mut bool,
    prev_match_requested: &mut bool,
    open_requested: &mut bool,
    follow_toggled: &mut bool,
    reverse_search: &mut bool,
    go_to_top: &mut bool,
    go_to_bottom: &mut bool,
) -> bool {
    // If keybind mode is not enabled, do nothing and return early.
    // This is important — we must not intercept keypresses when the user
    // is typing in a search box or other text input in normal GUI mode.
    // When search has focus, n/N should type into the search box, not navigate.
    if !state.enabled || search_focused {
        return false;
    }

    // ctx.input() provides a snapshot of all input events for the current frame.
    // The closure pattern (rather than a direct method call) is egui's way of
    // ensuring the input state is not accidentally held across frames.
    ctx.input(|input| {
        // --- j: scroll down one line ---
        // Equivalent to pressing the down arrow in less(1).
        // A positive scroll delta moves the viewport downward in egui.
        if input.key_pressed(egui::Key::J) {
            *scroll_delta = LINE_SCROLL_SPEED;
        }

        // --- k: scroll up one line ---
        // Equivalent to pressing the up arrow in less(1).
        // A negative scroll delta moves the viewport upward in egui.
        if input.key_pressed(egui::Key::K) {
            *scroll_delta = -LINE_SCROLL_SPEED;
        }

        // --- /: forward search mode ---
        // Focus the search input and set forward (normal) search direction.
        if input.key_pressed(egui::Key::Slash) && !input.modifiers.shift {
            *reverse_search = false;
            *search_focus_requested = true;
        }

        // --- ?: reverse search mode ---
        // Focus the search input and set reverse search direction.
        // n/N match cycling is swapped accordingly.
        if input.key_pressed(egui::Key::Slash) && input.modifiers.shift {
            *reverse_search = true;
            *search_focus_requested = true;
        }

        // --- f: page down ---
        // Moves down by one full viewport height, matching less(1) / more(1) behaviour.
        // viewport_rect().height() gives the total window height in logical pixels.
        // We use the full window height as a page size — a common terminal convention.
        let viewport_height = input.viewport_rect().height();
        if input.key_pressed(egui::Key::F) {
            *scroll_delta = viewport_height;
        }

        // --- b: page up ---
        // Moves up by one full viewport height, matching less(1) behaviour.
        if input.key_pressed(egui::Key::B) {
            *scroll_delta = -viewport_height;
        }

        // --- g: jump to top / first match ---
        // Sets scroll to the most negative possible value; egui clamps it to 0
        // (the top of the content) automatically. Also signals main.rs to
        // update current_match to the first search result.
        if input.key_pressed(egui::Key::G) && !input.modifiers.shift {
            *scroll_delta = f32::MIN;
            *go_to_top = true;
        }

        // --- G (shift+g): jump to bottom / last match ---
        // Sets scroll to the largest possible value; egui clamps it to the maximum
        // scroll position (total content height minus viewport height) automatically.
        // We check input.modifiers.shift to distinguish g from G.
        // Also signals main.rs to update current_match to the last search result.
        if input.key_pressed(egui::Key::G) && input.modifiers.shift {
            *scroll_delta = (total_rows as f32) * row_height;
            *go_to_bottom = true;
        }

        // --- o: open file ---
        // Opens a native file dialog to select a log file.
        if input.key_pressed(egui::Key::O) {
            *open_requested = true;
        }

        // --- t: toggle follow (tail -f) mode ---
        // Starts or stops watching the file for live updates.
        if input.key_pressed(egui::Key::T) && !input.modifiers.shift {
            *follow_toggled = true;
        }

        // --- n: next search match ---
        // Advances to the next search result (like n in vim/less).
        if input.key_pressed(egui::Key::N) && !input.modifiers.shift {
            *next_match_requested = true;
        }

        // --- N (shift+n): previous search match ---
        // Goes back to the previous search result (like N in vim/less).
        if input.key_pressed(egui::Key::N) && input.modifiers.shift {
            *prev_match_requested = true;
        }
    });

    // --- q: quit ---
    // Checked outside the input closure above because it returns a bool to main.rs
    // rather than mutating scroll_delta, and mixing return types inside the
    // ctx.input() closure would complicate the code unnecessarily.
    //
    // key_pressed() is queried via a second ctx.input() call — this is fine,
    // egui snapshots input once per frame so both calls see the same state.
    if ctx.input(|i| i.key_pressed(egui::Key::Q)) {
        // Signal to main.rs that the application should close.
        return true;
    }

    // No quit requested this frame.
    false
}
