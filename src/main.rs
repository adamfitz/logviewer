mod keybinds;

use eframe::egui;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use keybinds::KeybindState;
use notify::{self, EventKind, RecursiveMode, Watcher};
use rfd;

// Compute byte offsets for each line in the byte slice.
// Trailing newline does NOT add an extra empty line.
fn compute_line_offsets(content: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut pos = 0;
    while pos <= content.len() {
        match memchr::memchr(b'\n', &content[pos..]) {
            Some(nl) => {
                offsets.push(pos);
                pos += nl + 1;
            }
            None => {
                if pos < content.len() {
                    offsets.push(pos);
                }
                break;
            }
        }
    }
    offsets
}

fn load_emoji_font(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dejavu: &'static [u8] = include_bytes!("../assets/DejaVuSansMono.ttf");
    let emoji: &'static [u8] = include_bytes!("../assets/NotoEmoji-Regular.ttf");
    fonts.font_data.insert(
        "dejavu".into(),
        std::sync::Arc::new(egui::FontData::from_static(dejavu)),
    );
    fonts.font_data.insert(
        "emoji".into(),
        std::sync::Arc::new(egui::FontData::from_static(emoji)),
    );
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .clear();
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .extend(["dejavu".to_string(), "emoji".to_string()]);
    ctx.set_fonts(fonts);
}

use eframe::egui::containers::scroll_area::ScrollSource;

struct LogViewerApp {
    // Memory-mapped file content (either the original file or a decompressed temp).
    // None when no file is loaded.
    mmap: Option<memmap2::Mmap>,

    // Byte offset of each line's start within the mmap.
    // Offsets point to the first byte of each line (right after a newline, or 0).
    line_offsets: Vec<usize>,

    // Cached line count (line_offsets.len()).
    total_lines: usize,

    // Keeps the decompressed temp file alive while the mmap references it.
    // Dropped when a new file is loaded, which deletes the temp file on disk.
    // On Linux the mmap remains valid even after deletion, but keeping the
    // temp alive prevents surprises on other platforms.
    _temp_file: Option<tempfile::NamedTempFile>,

    // Error message to display instead of file content (e.g. decompression failure).
    error_message: Option<String>,

    keybind_state: KeybindState,
    search_query: String,
    search_focus_requested: bool,
    scroll_offset: f32,
    current_match: usize,
    file_path: Option<PathBuf>,
    open_requested: bool,
    following: bool,
    file_changed: Arc<AtomicBool>,
    _watcher: Option<notify::RecommendedWatcher>,
    follow_toggled: bool,
    font_size: f32,
    emoji_font_loaded: bool,
    from_compressed: bool,

    // Cached search results: indices into line_offsets of matching lines.
    // Non-empty only when search_query is non-empty.
    search_results: Vec<usize>,

    // Cached match positions for highlighting.
    // Each entry is (result_index, byte_offset_within_line).
    search_matches: Vec<(usize, usize)>,

    // The submitted search query (Enter pressed). Only this query triggers
    // search — typing in the search box alone does nothing until Enter.
    active_search_query: String,

    // True on the frame after search is submitted, so the scroll area
    // resets to the top of the filtered results.
    search_just_submitted: bool,
}

fn is_compressed(path: &PathBuf) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".tar.gz") || s.ends_with(".gz")
}

impl LogViewerApp {
    fn new(file_path: Option<PathBuf>) -> Self {
        let mut app = Self {
            mmap: None,
            line_offsets: Vec::new(),
            total_lines: 0,
            _temp_file: None,
            error_message: None,
            keybind_state: KeybindState::new(),
            search_query: String::new(),
            search_focus_requested: false,
            scroll_offset: 0.0,
            current_match: 0,
            file_path: None,
            open_requested: false,
            following: false,
            file_changed: Arc::new(AtomicBool::new(false)),
            _watcher: None,
            follow_toggled: false,
            font_size: 14.0,
            emoji_font_loaded: false,
            from_compressed: false,
            search_results: Vec::new(),
            search_matches: Vec::new(),
            active_search_query: String::new(),
            search_just_submitted: false,
        };
        if let Some(ref path) = file_path {
            app.load_file(path);
        }
        app
    }

    // Memory-map an uncompressed file.
    fn mmap_file(path: &PathBuf) -> Result<memmap2::Mmap, String> {
        let file = fs::File::open(path)
            .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        unsafe { memmap2::Mmap::map(&file) }
            .map_err(|e| format!("Failed to memory-map {}: {}", path.display(), e))
    }

    // Decompress a .gz to a temp file and return the mmap + optional temp handle.
    fn mmap_gz(path: &PathBuf) -> Result<(memmap2::Mmap, Option<tempfile::NamedTempFile>), String> {
        let file = fs::File::open(path)
            .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut temp = tempfile::NamedTempFile::new()
            .map_err(|_| "Failed to create temp file for decompression".to_string())?;
        std::io::copy(&mut decoder, temp.as_file_mut())
            .map_err(|e| format!("Failed to decompress {}: {}", path.display(), e))?;
        temp.flush().unwrap();
        let mmap = unsafe { memmap2::Mmap::map(temp.as_file()) }
            .map_err(|e| format!("Failed to memory-map decompressed file: {}", e))?;
        Ok((mmap, Some(temp)))
    }

    // Decompress a .tar.gz to a temp file and return the mmap + optional temp handle.
    // Extracts the first entry in the archive.
    fn mmap_tar_gz(
        path: &PathBuf,
    ) -> Result<(memmap2::Mmap, Option<tempfile::NamedTempFile>), String> {
        let file = fs::File::open(path)
            .map_err(|e| format!("Failed to open archive: {}\nError: {}", path.display(), e))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        let mut temp =
            tempfile::NamedTempFile::new().map_err(|_| "Failed to create temp file".to_string())?;
        let entries = archive
            .entries()
            .map_err(|_| "Failed to read archive entries".to_string())?;
        let mut extracted = false;
        for result in entries {
            let mut entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !extracted {
                std::io::copy(&mut entry, temp.as_file_mut())
                    .map_err(|_| "Failed to extract archive entry".to_string())?;
                temp.flush().unwrap();
                extracted = true;
                break;
            }
        }
        if !extracted {
            return Err("Archive is empty".to_string());
        }
        let mmap = unsafe { memmap2::Mmap::map(temp.as_file()) }
            .map_err(|e| format!("Failed to memory-map decompressed file: {}", e))?;
        Ok((mmap, Some(temp)))
    }

    fn load_file(&mut self, path: &PathBuf) {
        self.stop_following();

        // Clear previous state.
        self.mmap = None;
        self._temp_file = None;
        self.line_offsets.clear();
        self.search_results.clear();
        self.search_matches.clear();
        self.active_search_query.clear();
        self.error_message = None;

        let compressed = is_compressed(path);
        self.from_compressed = compressed;

        let result = if compressed {
            if path.to_string_lossy().ends_with(".tar.gz") {
                Self::mmap_tar_gz(path)
            } else {
                Self::mmap_gz(path)
            }
        } else {
            Self::mmap_file(path).map(|m| (m, None))
        };

        match result {
            Ok((mmap, temp_file)) => {
                self.line_offsets = compute_line_offsets(&mmap);
                self.total_lines = self.line_offsets.len();
                self.mmap = Some(mmap);
                self._temp_file = temp_file;
                self.file_path = Some(path.clone());
            }
            Err(err) => {
                self.error_message = Some(err);
                self.file_path = Some(path.clone());
            }
        }

        self.search_query.clear();
        self.active_search_query.clear();
        self.current_match = 0;
        self.scroll_offset = 0.0;
        self.search_just_submitted = false;
    }

    fn start_following(&mut self) {
        if self.following || self.file_path.is_none() || self.from_compressed {
            return;
        }
        let path = self.file_path.as_ref().unwrap().clone();
        let changed = self.file_changed.clone();
        let mut watcher =
            match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(event.kind, EventKind::Modify(_)) {
                        changed.store(true, Ordering::Relaxed);
                    }
                }
            }) {
                Ok(w) => w,
                Err(_) => return,
            };
        if watcher.watch(&path, RecursiveMode::NonRecursive).is_ok() {
            self._watcher = Some(watcher);
            self.following = true;
        }
    }

    fn stop_following(&mut self) {
        self._watcher = None;
        self.following = false;
    }

    // Rebuild cached search results by scanning the mmap with memchr.
    fn rebuild_search(&mut self) {
        self.search_results.clear();
        self.search_matches.clear();
        self.current_match = 0;

        let query = self.active_search_query.as_bytes();
        if query.is_empty() || self.mmap.is_none() {
            return;
        }

        let mmap = self.mmap.as_ref().unwrap();
        let finder = memchr::memmem::Finder::new(query);
        let mut fi = 0usize;

        for (&start, line_idx) in self.line_offsets.iter().zip(0..) {
            let end = if line_idx + 1 < self.total_lines {
                // Exclude the newline byte
                self.line_offsets[line_idx + 1] - 1
            } else {
                // Last line: exclude trailing newline if present
                let raw_end = mmap.len();
                if raw_end > 0 && mmap[raw_end - 1] == b'\n' {
                    raw_end - 1
                } else {
                    raw_end
                }
            };
            let line = &mmap[start..end];

            if finder.find(line).is_some() {
                self.search_results.push(line_idx);

                let mut search_pos = 0;
                while let Some(rel_pos) = finder.find(&line[search_pos..]) {
                    self.search_matches.push((fi, search_pos + rel_pos));
                    search_pos += rel_pos + query.len();
                }
                fi += 1;
            }
        }
    }
}

impl eframe::App for LogViewerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

        ui.style_mut().text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::monospace(self.font_size),
        );

        let frame_fill = if self.keybind_state.enabled {
            egui::Color32::BLACK
        } else {
            egui::Color32::WHITE
        };
        let frame = egui::Frame::new().fill(frame_fill);

        if !self.emoji_font_loaded {
            load_emoji_font(ui.ctx());
            self.emoji_font_loaded = true;
        }

        let title = if let Some(ref path) = self.file_path {
            format!(
                "Log Viewer - {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            "Log Viewer".to_string()
        };
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(title));

        // GUI-mode keybinds
        if !self.keybind_state.enabled {
            if ui.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.ctrl) {
                std::process::exit(0);
            }
            if ui.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.ctrl) {
                self.open_requested = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::W) && i.modifiers.ctrl) {
                self.follow_toggled = true;
            }
        }

        if self.follow_toggled {
            self.follow_toggled = false;
            if self.file_path.is_some() && !self.from_compressed {
                if self.following {
                    self.stop_following();
                } else {
                    self.start_following();
                }
            }
        }

        // Follow-mode file reload: re-mmap the file and recompute lines.
        if self.following && self.file_changed.swap(false, Ordering::Relaxed) {
            if let Some(ref path) = self.file_path.clone() {
                if let Ok(file) = fs::File::open(path) {
                    if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
                        self.line_offsets = compute_line_offsets(&mmap);
                        self.total_lines = self.line_offsets.len();
                        self.mmap = Some(mmap);
                        self.search_query.clear();
                        self.active_search_query.clear();
                        self.search_results.clear();
                        self.search_matches.clear();
                        self.search_just_submitted = false;
                    }
                }
            }
        }

        if self.open_requested {
            self.open_requested = false;
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Log files (*.log, *.txt, *.gz, *.tar.gz)",
                    &[
                        "log", "txt", "out", "err", "stdout", "stderr", "gz", "tar.gz",
                    ],
                )
                .pick_file()
            {
                self.load_file(&path);
            }
        }

        frame.show(ui, |ui| {
            // --- Menu bar ---
            egui::MenuBar::new().ui(ui, |ui| {
                let menu_item =
                    |ui: &mut egui::Ui,
                     label: &str,
                     shortcut: &str,
                     action: &mut dyn FnMut(&mut egui::Ui)| {
                        let inner = ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(label).size(16.0));
                            ui.add_space(48.0);
                            ui.weak(egui::RichText::new(shortcut).size(14.0));
                        });
                        let response = ui.interact(
                            inner.response.rect,
                            inner.response.id.with(label),
                            egui::Sense::click(),
                        );
                        if response.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                            let hover_color =
                                egui::Color32::from_rgba_premultiplied(128, 128, 128, 60);
                            ui.painter().rect_filled(response.rect, 4.0, hover_color);
                        }
                        if response.clicked() {
                            action(ui);
                        }
                    };

                ui.menu_button(egui::RichText::new("File").size(16.0), |ui| {
                    menu_item(ui, "Open...", "Ctrl+O", &mut |ui| {
                        self.open_requested = true;
                        ui.close();
                    });

                    menu_item(ui, "Quit", "Ctrl+Q", &mut |_ui| {
                        std::process::exit(0);
                    });
                });

                ui.menu_button(egui::RichText::new("Tools").size(16.0), |ui| {
                    let follow_label = if self.following {
                        "Stop Following"
                    } else {
                        "Follow (tail -f)"
                    };
                    let follow_shortcut = if self.keybind_state.enabled {
                        "t"
                    } else {
                        "Ctrl+W"
                    };
                    menu_item(ui, follow_label, follow_shortcut, &mut |menu_ui| {
                        self.follow_toggled = true;
                        menu_ui.close();
                    });

                    ui.separator();

                    ui.menu_button(egui::RichText::new("Font").size(16.0), |ui| {
                        let sizes = [(14.0f32, "Small"), (18.0, "Medium"), (22.0, "Large")];
                        for &(size, label) in &sizes {
                            let is_current = (self.font_size - size).abs() < 0.01;
                            let inner = ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(label).size(16.0));
                                ui.add_space(48.0);
                                ui.weak(
                                    egui::RichText::new(if is_current { "*" } else { "" })
                                        .size(14.0),
                                );
                            });
                            let response = ui.interact(
                                inner.response.rect,
                                inner.response.id.with(label),
                                egui::Sense::click(),
                            );
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                                let hover_color =
                                    egui::Color32::from_rgba_premultiplied(128, 128, 128, 60);
                                ui.painter().rect_filled(response.rect, 4.0, hover_color);
                            }
                            if response.clicked() {
                                self.font_size = size;
                                ui.close();
                            }
                        }
                    });
                });
            });

            // --- Header bar ---
            let search_response = ui.horizontal(|ui| {
                ui.set_min_height(32.0);
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
                    if self.following {
                        ui.label(
                            egui::RichText::new(" ● Follow")
                                .size(14.0)
                                .color(egui::Color32::LIGHT_GREEN),
                        );
                    }
                });
                response
            });

            ui.separator();

            let search_text_response = search_response.inner;
            let search_has_focus = search_text_response.has_focus();

            if self.search_focus_requested {
                search_text_response.request_focus();
                self.search_focus_requested = false;
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

            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);

            // Enter / Shift+Enter: submit new query or cycle through matches.
            // TextEdit::singleline surrenders focus on Enter, so we check the
            // Enter key globally (not guarded by search_has_focus) so that
            // submission works. Cycling still requires focus.
            let (enter, shift) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift));
            if enter {
                let new_query = self.search_query != self.active_search_query;
                if new_query {
                    self.active_search_query = self.search_query.clone();
                    if self.active_search_query.is_empty() {
                        self.search_results.clear();
                        self.search_matches.clear();
                    } else if self.mmap.is_some() {
                        self.rebuild_search();
                    }
                    self.current_match = 0;
                    self.search_just_submitted = true;
                    self.scroll_offset = 0.0;
                } else if search_has_focus && !self.search_matches.is_empty() {
                    if shift {
                        self.current_match = if self.current_match == 0 {
                            self.search_matches.len() - 1
                        } else {
                            self.current_match - 1
                        };
                    } else {
                        self.current_match = (self.current_match + 1) % self.search_matches.len();
                    }
                    let (fi, _) = self.search_matches[self.current_match];
                    self.scroll_offset = fi as f32 * row_height;
                }
            }

            // Compute these AFTER the Enter handler so clearing the search
            // (which empties search_results / search_matches) is reflected.
            let has_query = !self.active_search_query.is_empty();
            let total_rows = if has_query {
                self.search_results.len()
            } else {
                self.total_lines
            };
            if self.current_match >= self.search_matches.len() {
                self.current_match = 0;
            }

            // Terminal keybinds
            let mut next_match = false;
            let mut prev_match = false;
            let mut follow_toggled = false;
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
                &mut self.open_requested,
                &mut follow_toggled,
            );

            if next_match && !self.search_matches.is_empty() {
                self.current_match = (self.current_match + 1) % self.search_matches.len();
                let (fi, _) = self.search_matches[self.current_match];
                self.scroll_offset = fi as f32 * row_height;
            }
            if prev_match && !self.search_matches.is_empty() {
                self.current_match = if self.current_match == 0 {
                    self.search_matches.len() - 1
                } else {
                    self.current_match - 1
                };
                let (fi, _) = self.search_matches[self.current_match];
                self.scroll_offset = fi as f32 * row_height;
            }

            self.follow_toggled = self.follow_toggled || follow_toggled;

            if should_quit {
                std::process::exit(0);
            }

            if !self.keybind_state.enabled
                && ui.input(|i| i.key_pressed(egui::Key::F) && i.modifiers.ctrl)
            {
                self.search_focus_requested = true;
            }

            if !self.keybind_state.enabled && !self.search_matches.is_empty() {
                let (down, up) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::ArrowDown) && i.modifiers.ctrl,
                        i.key_pressed(egui::Key::ArrowUp) && i.modifiers.ctrl,
                    )
                });
                if down {
                    self.current_match = (self.current_match + 1) % self.search_matches.len();
                    let (fi, _) = self.search_matches[self.current_match];
                    self.scroll_offset = fi as f32 * row_height;
                }
                if up {
                    self.current_match = if self.current_match == 0 {
                        self.search_matches.len() - 1
                    } else {
                        self.current_match - 1
                    };
                    let (fi, _) = self.search_matches[self.current_match];
                    self.scroll_offset = fi as f32 * row_height;
                }
            }

            ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
            let mut scroll_area = egui::ScrollArea::vertical()
                .auto_shrink(false)
                .stick_to_bottom(self.following)
                .scroll_source(ScrollSource::SCROLL_BAR | ScrollSource::MOUSE_WHEEL);
            if self.scroll_offset != 0.0 || self.search_just_submitted {
                scroll_area = scroll_area.vertical_scroll_offset(self.scroll_offset);
                self.scroll_offset = 0.0;
                self.search_just_submitted = false;
            }

            // Pre-compute text formats for search highlighting
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

            // --- Main content area ---
            let has_content = self.mmap.is_some() || self.error_message.is_some();

            if !has_content && self.file_path.is_none() {
                // Welcome screen
                let fill_rect = ui.available_rect_before_wrap();
                ui.painter().rect_filled(fill_rect, 0.0, frame_fill);
                ui.scope_builder(egui::UiBuilder::new().max_rect(fill_rect), |ui| {
                    let content_height = 60.0;
                    let top_padding = ((fill_rect.height() - content_height) / 2.0).max(0.0);
                    ui.add_space(top_padding);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Welcome to Log Viewer")
                                .size(28.0)
                                .color(normal_color)
                                .monospace(),
                        );
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new("Use File > Open or Ctrl+O to open a log file")
                                .size(16.0)
                                .color(normal_color)
                                .monospace(),
                        );
                    });
                });
            } else if let Some(ref err) = self.error_message.clone() {
                // Error state
                let fill_rect = ui.available_rect_before_wrap();
                ui.painter().rect_filled(fill_rect, 0.0, frame_fill);
                ui.scope_builder(egui::UiBuilder::new().max_rect(fill_rect), |ui| {
                    let content_height = 60.0;
                    let top_padding = ((fill_rect.height() - content_height) / 2.0).max(0.0);
                    ui.add_space(top_padding);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("Error loading file")
                                .size(20.0)
                                .color(egui::Color32::RED)
                                .monospace(),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(err)
                                .size(14.0)
                                .color(normal_color)
                                .monospace(),
                        );
                    });
                });
            } else {
                // Normal log file rendering
                let mmap = self.mmap.as_ref().unwrap();
                let line_offsets = &self.line_offsets;
                let total_lines = self.total_lines;
                let search_results = &self.search_results;
                let search_matches = &self.search_matches;
                let active_query = self.active_search_query.clone();

                if has_query && total_rows == 0 {
                    let fill_rect = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(fill_rect, 0.0, frame_fill);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(fill_rect), |ui| {
                        let top_padding = ((fill_rect.height() - 30.0) / 2.0).max(0.0);
                        ui.add_space(top_padding);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                egui::RichText::new("No matches found")
                                    .size(18.0)
                                    .color(normal_color)
                                    .monospace(),
                            );
                        });
                    });
                } else {
                    scroll_area.show_rows(ui, row_height, total_rows, |ui, row_range| {
                        for abs_fi in row_range {
                            let line_idx = if has_query {
                                search_results[abs_fi]
                            } else {
                                abs_fi
                            };

                            let start = line_offsets[line_idx];
                            let end = if line_idx + 1 < total_lines {
                                line_offsets[line_idx + 1] - 1
                            } else {
                                let raw_end = mmap.len();
                                if raw_end > 0 && mmap[raw_end - 1] == b'\n' {
                                    raw_end - 1
                                } else {
                                    raw_end
                                }
                            };

                            let line_bytes = &mmap[start..end];
                            let line = String::from_utf8_lossy(line_bytes);

                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    egui::vec2(72.0, row_height),
                                    egui::Label::new(
                                        egui::RichText::new(format!("{:>6}", line_idx + 1))
                                            .monospace(),
                                    ),
                                );
                                if active_query.is_empty() {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(line.as_ref()).monospace(),
                                        )
                                        .wrap(),
                                    );
                                } else {
                                    let mut job = egui::text::LayoutJob::default();
                                    let line_str = line.as_ref();
                                    let mut start = 0;
                                    while let Some(pos) = line_str[start..].find(&active_query) {
                                        let abs_pos = start + pos;
                                        if abs_pos > start {
                                            job.append(
                                                &line_str[start..abs_pos],
                                                0.0,
                                                normal_fmt.clone(),
                                            );
                                        }
                                        let is_current = search_matches
                                            .get(self.current_match)
                                            .map(|&(cm_fi, cm_off)| {
                                                cm_fi == abs_fi && cm_off == abs_pos
                                            })
                                            .unwrap_or(false);
                                        let fmt =
                                            if is_current { &current_fmt } else { &match_fmt };
                                        job.append(
                                            &line_str[abs_pos..abs_pos + active_query.len()],
                                            0.0,
                                            fmt.clone(),
                                        );
                                        start = abs_pos + active_query.len();
                                    }
                                    if start < line_str.len() {
                                        job.append(&line_str[start..], 0.0, normal_fmt.clone());
                                    }
                                    ui.add(egui::Label::new(job).wrap());
                                }
                            });
                        }
                    });
                }
            }
        });
    }
}

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = env::args().collect();
    let file_path = if args.len() > 1 {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Log Viewer",
        options,
        Box::new(|_cc| Ok(Box::new(LogViewerApp::new(file_path)))),
    )
}
