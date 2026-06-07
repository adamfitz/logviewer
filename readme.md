# rlv — Rust Log Viewer

A standalone text file log viewer built with Rust and `eframe`/`egui`. Can be launched independently or as a child process with an optional log file path.

## Features

- **GUI framework** — Cross-platform native window via `eframe` + `egui` (immediate-mode, zero runtime deps)
- **Memory-mapped file I/O** — Files are memory-mapped (not read into `Vec<String>`); only visible lines are extracted on demand. Line offsets stored as a compact `Vec<usize>` (~8 bytes per line). No full-file allocation.
- **Virtual scrolling** — Only visible lines are rendered per frame via `egui::ScrollArea::show_rows()`, handling large files smoothly
- **PCRE-compatible regex search** — Search patterns are treated as regular expressions (via `fancy-regex`). Supports lookahead/lookbehind, alternation, character classes, etc. Invalid patterns show an error message; "No matches found" shown when no lines match.
- **Enter-to-submit search** — Typing does nothing until Enter is pressed. First Enter submits the query; subsequent Enters cycle through matches.
- **Incremental batched search** — Searches over 100,000 lines per frame in the background so the UI stays responsive. A "Searching... XX%" progress indicator is shown while the search runs.
- **Reverse search (terminal mode)** — Press `?` to search backwards; match cycling with `n`/`N`, Enter, and Shift+Enter is reversed. The initial match lands on the last result instead of the first.
- **Search match cycling** — Forward/backward navigation through matches:
  - **GUI mode:** Enter / Shift+Enter (search focused), Ctrl+Down / Ctrl+Up (anywhere)
  - **Terminal mode:** `n` / `N` (when search is not focused), Enter / Shift+Enter (search focused)
- **Current match highlight** — The active match uses a high-contrast bright gold background + black text to visually distinguish it from other matches
- **Terminal keybind mode** — vi/less-style navigation (`j`/`k` scroll, `f`/`b` page, `g`/`G` top/bottom, `/` search, `o` open file, `t` follow toggle, `q` quit)
- **Mode toggle** — Switch between GUI and Terminal mode via a button in the top bar
- **Dark / light themes** — Terminal mode uses dark theme with black background; GUI mode uses light theme with white background
- **Tail -f live follow** — Watch the file for changes and stream new lines in real time (toggle via Tools menu, Ctrl+W, or `t`)
- **Compressed file support** — Open `.gz` and `.tar.gz` files; decompressed to temp files then memory-mapped
- **Hover-to-open menus** — File and Tools dropdowns open on hover with click-outside-to-close; menu items have full-width blue hover highlight with right-aligned keybinds
- **Font size adjustment** — Tools > Font opens a flyout submenu with Small (14px), Medium (18px), Large (22px); selected size marked with `*`; accessible via Ctrl+S in both GUI and terminal modes
- **Optional CLI argument** — Start without a file to see the welcome screen, or pass a path to open directly
- **Dynamic window title** — Shows the current file name in the title bar
- **Text selection + clipboard copy** — Text in log lines is selectable; right-click on any line copies it to the system clipboard. In terminal mode, `yy` yanks (copies) the current line.
- **Bundled emoji font** — DejaVu Sans Mono (primary, broad Unicode coverage) + Noto Emoji (fallback for emoji glyphs)

## Usage

```
rlv                               # start empty, use File > Open
rlv /var/log/syslog               # open a file directly
rlv logfile.gz                    # open a compressed file (.gz / .tar.gz)
```

### Keybinds

#### GUI Mode (default)

| Key | Context | Action |
|---|---|---|
| `Ctrl+O` | Anywhere | Open file |
| `Ctrl+Q` | Anywhere | Quit |
| `Ctrl+W` | Anywhere | Toggle follow |
| `Ctrl+S` | Anywhere | Open font size menu |
| `Ctrl+F` | Anywhere | Focus search box |
| `Enter` | Search focused | Submit query / Next match |
| `Shift+Enter` | Search focused | Submit query / Previous match |
| `Ctrl+Down` | Anywhere | Next match (if search active) |
| `Ctrl+Up` | Anywhere | Previous match (if search active) |
| `Ctrl+C` | Anywhere | Copy selected text |
| Right-click | On a line | Copy that line to clipboard |

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
| `/` | Anywhere | Focus search box (forward) |
| `?` | Anywhere | Focus search box (reverse) |
| `o` | Anywhere | Open file |
| `yy` (double-tap `y`) | Anywhere | Yank (copy) current line to clipboard |
| `t` | Anywhere | Toggle follow |
| `Ctrl+S` | Anywhere | Open font size menu |
| `q` | Anywhere | Quit |
| `Enter` | Search focused | Next match |
| `Shift+Enter` | Search focused | Previous match |
| `n` | Search not focused | Next match |
| `N` (Shift+n) | Search not focused | Previous match |

## Acknowledgments

This application bundles the following open-source fonts:

- **DejaVu Sans Mono** — Copyright © 2003 Bitstream, Inc.  
  DejaVu changes are in the public domain.  
  https://dejavu-fonts.github.io/

- **Noto Emoji** — Copyright © 2013 Google Inc.  
  Licensed under the SIL Open Font License, Version 1.1.  
  https://fonts.google.com/noto
