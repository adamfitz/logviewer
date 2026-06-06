# Log Viewer

A standalone text file log viewer built with Rust and `eframe`/`egui`. Launched as an independent child process from a parent application.

## Features

### Implemented

- **GUI framework** — Cross-platform native window via `eframe` + `egui` (immediate-mode, zero runtime deps)
- **Virtual scrolling** — Only visible lines are rendered per frame via `egui::ScrollArea::show_rows()`, handling large files smoothly
- **Search** — Plain substring search with a visible text box; matches highlighted inline with coloured backgrounds
- **Search match cycling** — Forward/backward navigation through matches:
  - **GUI mode:** Enter / Shift+Enter (search focused), Ctrl+Down / Ctrl+Up (anywhere)
  - **Terminal mode:** `n` / `N` (when search is not focused), Enter / Shift+Enter (search focused)
- **Current match highlight** — The active match uses a high-contrast bright gold background + black text to visually distinguish it from other matches
- **Terminal keybind mode** — vi/less-style navigation (`j`/`k` scroll, `f`/`b` page, `g`/`G` top/bottom, `/` search, `q` quit)
- **Mode toggle** — Switch between GUI and Terminal mode via a button in the top bar
- **Dark / light themes** — Terminal mode uses dark theme with black background; GUI mode uses light theme with white background
- **Resize handling** — Window resizes correctly without crashing; row positions are recalculated per frame

### Planned / Not Yet Implemented

| Feature | Dependencies | Notes |
|---|---|---|
| **Memory-mapped file I/O** | `memmap2` | Replace `Vec<String>` with a `Vec<u64>` of newline offsets; only read visible lines from disk |
| **tail -f (live follow)** | `notify` | Watch file for append events and stream new lines in real time |
| **Regex search** | `fancy-regex` | PCRE-compatible search with lookahead/lookbehind support |
| **Reverse search** | — | `?` keybind in terminal mode for backward search |
| **.tar.gz support** | `flate2` + `tar` | Decompress and view `.tar.gz` log archives |
| **Text selection + clipboard copy** | `arboard` | Ctrl+C to copy selected text to system clipboard |
| **File open dialog** | `rfd` | Native file picker to open a different log file without restarting |
