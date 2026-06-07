# Log Viewer

A standalone text file log viewer built with Rust and `eframe`/`egui`. Can be launched independently or as a child process with an optional log file path.

## Features

- **GUI framework** — Cross-platform native window via `eframe` + `egui` (immediate-mode, zero runtime deps)
- **Virtual scrolling** — Only visible lines are rendered per frame via `egui::ScrollArea::show_rows()`, handling large files smoothly
- **Search** — Plain substring search with a visible text box; matches highlighted inline with coloured backgrounds
- **Search match cycling** — Forward/backward navigation through matches:
  - **GUI mode:** Enter / Shift+Enter (search focused), Ctrl+Down / Ctrl+Up (anywhere)
  - **Terminal mode:** `n` / `N` (when search is not focused), Enter / Shift+Enter (search focused)
- **Current match highlight** — The active match uses a high-contrast bright gold background + black text to visually distinguish it from other matches
- **Terminal keybind mode** — vi/less-style navigation (`j`/`k` scroll, `f`/`b` page, `g`/`G` top/bottom, `/` search, `o` open file, `q` quit)
- **Mode toggle** — Switch between GUI and Terminal mode via a button in the top bar
- **Dark / light themes** — Terminal mode uses dark theme with black background; GUI mode uses light theme with white background
- **File menu** — File > Open (native file dialog) and File > Quit
- **Optional CLI argument** — Start without a file to see the welcome screen, or pass a path to open directly
- **Dynamic window title** — Shows the current file name in the title bar

### Planned / Not Yet Implemented

| Feature | Dependencies | Notes |
|---|---|---|
| **Memory-mapped file I/O** | `memmap2` | Replace `Vec<String>` with a `Vec<u64>` of newline offsets; only read visible lines from disk |
| **tail -f (live follow)** | `notify` | Watch file for append events and stream new lines in real time |
| **Regex search** | `fancy-regex` | PCRE-compatible search with lookahead/lookbehind support |
| **Reverse search** | — | `?` keybind in terminal mode for backward search |
| **.tar.gz support** | `flate2` + `tar` | Decompress and view `.tar.gz` log archives |
| **Text selection + clipboard copy** | `arboard` | Ctrl+C to copy selected text to system clipboard |

## Usage

```
logviewer                  # start empty, use File > Open
logviewer <path/to/logfile.log>  # open directly
```

### Keybinds

#### GUI Mode (default)

| Key | Context | Action |
|---|---|---|
| `Ctrl+O` | Anywhere | Open file |
| `Ctrl+Q` | Anywhere | Quit |
| `Ctrl+F` | Anywhere | Focus search box |
| `Enter` | Search focused | Next match |
| `Shift+Enter` | Search focused | Previous match |
| `Ctrl+Down` | Anywhere | Next match |
| `Ctrl+Up` | Anywhere | Previous match |

#### Terminal Mode (vim/less-style)

Toggle on via the **"Mode: Terminal (vim/less)"** button in the top bar.

| Key | Context | Action |
|---|---|---|
| `j` | Anywhere | Scroll down one line |
| `k` | Anywhere | Scroll up one line |
| `f` | Anywhere | Page down |
| `b` | Anywhere | Page up |
| `g` | Anywhere | Jump to top |
| `G` (Shift+g) | Anywhere | Jump to bottom |
| `/` | Anywhere | Focus search box |
| `o` | Anywhere | Open file |
| `q` | Anywhere | Quit |
| `Enter` | Search focused | Next match |
| `Shift+Enter` | Search focused | Previous match |
| `n` | Search not focused | Next match |
| `N` (Shift+n) | Search not focused | Previous match |
