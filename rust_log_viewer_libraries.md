# Rust Log Viewer — Library Recommendations

## Why `eframe`/`egui` over GTK/GIO

GTK/GIO is a valid choice but has real drawbacks for this use case:

- The Rust bindings (`gtk4-rs`) are significantly more complex to set up
- GTK requires runtime libraries to be present on the system — painful on Windows where GTK is not native
- The binding API is verbose and less idiomatic Rust
- `eframe`/`egui` is pure Rust, statically linkable, ships a single binary with **zero runtime dependencies**, and works identically on Windows and Linux out of the box

For a utility spawned from another application, zero-dependency deployment wins every time. GTK would be the right call if you needed deep desktop integration (system tray, DBus, GNOME HIG compliance) — this use case does not.

---

## Architecture Overview

The Rust log viewer is a **standalone binary** launched as a child process from the Fyne application.

**From the Go/Fyne side:**
```go
cmd := exec.Command("/path/to/logviewer", "--file", selectedLogPath)
cmd.Start() // fire and forget — completely independent process
```

The Rust app owns its own window, its own event loop, and cannot crash the Fyne application.

---

## Requirements → Library Mapping

| Requirement | Library | Notes |
|---|---|---|
| **GUI framework** | `eframe` + `egui` | Immediate-mode GUI, cross-platform, zero runtime deps |
| **Byte streaming / memory efficiency** | `memmap2` | Memory-mapped file I/O — reads chunks without loading full file |
| | `std::io::BufReader` | Stdlib buffered reading for sequential streaming |
| **tail -f (live follow)** | `notify` | Cross-platform filesystem watcher (inotify on Linux, ReadDirectoryChangesW on Windows) |
| **Search forward/reverse** | `regex` | RE2-syntax engine, fast, safe |
| **PCRE regex (grep-compatible)** | `fancy-regex` | Adds lookahead/lookbehind on top of `regex`; pure Rust, no system deps |
| **Highlight search matches** | `egui` built-in | Use `egui::text::LayoutJob` with `TextFormat` to colour spans inline |
| **.tar.gz support** | `flate2` + `tar` | `flate2` for gzip decompression, `tar` for archive traversal |
| **Text selection + Ctrl+C copy** | `egui` built-in + `arboard` | `egui` handles selection; `arboard` is the cross-platform clipboard crate |
| **Open new file / close current** | `rfd` | Rusty File Dialog — native open-file dialog on both platforms |
| **Resize without crash** | `eframe` built-in | `eframe` handles window resize events natively |
| **Separate process** | OS process spawn (Go side) | Rust binary is self-contained; Fyne calls `exec.Command` |

---

## `Cargo.toml` Dependencies

```toml
[dependencies]
# GUI
eframe = "0.34"
egui = "0.34"

# File watching (tail -f)
notify = "8"

# Regex — PCRE-compatible
fancy-regex = "0.14"

# Memory-mapped file streaming
memmap2 = "0.9"

# .tar.gz support
flate2 = "1"
tar = "0.4"

# Clipboard (Ctrl+C copy)
arboard = "3"

# Native file open dialog
rfd = "0.17"
```

---

## Key Design Notes

### Memory Efficiency

Use `memmap2` to map the file and maintain a list of **line byte offsets** — just a `Vec<u64>` of newline positions. You never hold line content in memory. You seek to the offset and read only what is visible on screen.

For a 10 GB log file, the offset index is a few tens of MB at most. The visible viewport reads a few KB at a time.

### tail -f Implementation

`notify` watches the file for write events. On each event, read only the new bytes appended past the last known file size:

```rust
file.seek(SeekFrom::Start(last_known_size))?;
// read new lines from here
```

### .tar.gz Files

Decompress to a temp file first using `flate2` + `tar` (streaming decompression), then open the extracted file normally with `memmap2`. Do not attempt to memory-map a compressed stream — it will not work.

### PCRE vs `fancy-regex`

`fancy-regex` is the easier path — pure Rust, no system dependencies, supports lookahead, lookbehind, and backreferences. Only reach for the `pcre2` crate if you hit a specific grep pattern that `fancy-regex` cannot handle (rare in practice).

### Search Highlighting

Use `egui::text::LayoutJob` to build text with per-character formatting. Match spans get a different `TextFormat` (background colour, bold, etc.). Walk forward/backward through a `Vec<(usize, usize)>` of match byte ranges to implement next/previous navigation.

### Text Selection and Copy

`egui`'s `TextEdit` widget handles mouse-drag selection natively. Wire `arboard::Clipboard::set_text()` to the Ctrl+C event to copy the selected range to the system clipboard.

---

## Optional: Terminal Mode (Nice to Have)

If you want to offer a "terminal keybinds" mode (vi/less-style navigation), implement a secondary input handler that intercepts:

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll down / up one line |
| `g` / `G` | Jump to top / bottom |
| `/` | Enter search (forward) |
| `?` | Enter search (reverse) |
| `n` / `N` | Next / previous match |
| `f` / `b` | Page down / page up |
| `q` | Quit |

No additional library is required — `egui` exposes raw key events via `ctx.input(|i| i.key_pressed(...))`. Toggle between GUI mode and terminal mode with a button or menu item.

---

## Summary

| Category | Chosen Library | Reason |
|---|---|---|
| GUI | `eframe` + `egui` | Zero deps, pure Rust, cross-platform |
| File watch | `notify` | Only cross-platform watcher in the Rust ecosystem |
| Regex | `fancy-regex` | PCRE-compatible, no system libs needed |
| Memory-mapped I/O | `memmap2` | Efficient large file handling |
| Archive | `flate2` + `tar` | Standard Rust ecosystem choice |
| Clipboard | `arboard` | Only serious cross-platform clipboard crate |
| File dialog | `rfd` | Native dialogs, well maintained |
