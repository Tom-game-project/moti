use crate::buffer::Buffer;
use crate::mode::Mode;
use crate::plugin::{PluginEffect, PluginManager};
use crate::syntax::SyntaxStyle;
use crate::tree::{self, TreeItem};
use crate::ui;
use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyEventKind},
    execute,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    Terminal,
};
use std::{
    collections::HashSet,
    io,
    path::PathBuf,
    time::Duration,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub struct Editor {
    pub buffers: Vec<Buffer>,
    pub active_buffer_index: usize,
    pub mode: Mode,
    pub command_input: String,
    pub command_message: String,
    pub scroll_offset_col: usize,
    pub should_exit: bool,
    pub pending_command_prefix: Option<char>,
    pub plugin_manager: PluginManager,
    pub plugin_event_receiver: Receiver<PluginEffect>,
    pub plugin_event_sender: Sender<PluginEffect>,
    pub tree_visible: bool,
    pub tree_view_active: bool,
    pub tree_width: u16,
    pub current_path: PathBuf,
    pub tree_scroll_pos: usize,
    pub selected_item_index: usize,
    pub expanded_dirs: HashSet<PathBuf>,
    pub tree_items: Vec<TreeItem>,
}

impl Editor {
    pub fn new() -> Result<Self> {
        let (tx, rx) = unbounded();
        let editor = Editor {
            buffers: Vec::new(),
            active_buffer_index: 0,
            mode: Mode::Normal,
            command_input: String::new(),
            command_message: String::new(),
            scroll_offset_col: 0,
            should_exit: false,
            pending_command_prefix: None,
            plugin_manager: PluginManager::new()?,
            plugin_event_receiver: rx,
            plugin_event_sender: tx,
            tree_visible: true,
            tree_view_active: true,
            tree_width: 30,
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            tree_scroll_pos: 0,
            selected_item_index: 0,
            expanded_dirs: HashSet::new(),
            tree_items: Vec::new(),
        };
        Ok(editor)
    }

    pub fn setup(&mut self) {
        self.expanded_dirs.insert(self.current_path.clone());
        self.open_file_in_new_buffer(None);
        self.command_message.clear();
        self.load_plugins();
    }

    fn load_plugins(&mut self) {
        let plugin_path = PathBuf::from("./plugin.wasm");
        if plugin_path.exists() {
            if let Err(e) = self.plugin_manager.load_plugin(&plugin_path, self.plugin_event_sender.clone()) {
                self.command_message = format!("Failed to load plugin: {}", e);
            }
        } else {
            self.command_message = "plugin.wasm not found. Skipping plugin load.".to_string();
        }
    }

    fn handle_plugin_events(&mut self) {
        while let Ok(effect) = self.plugin_event_receiver.try_recv() {
            match effect {
                PluginEffect::Echo(message) => {
                    self.command_message = message;
                }
                PluginEffect::ApplyTextStyle { line, start_byte, end_byte, style_id } => {
                    if let Some(buffer) = self.active_buffer() {
                        let highlights = buffer.highlights.entry(line as usize).or_default();
                        highlights.push((
                            (start_byte as usize)..(end_byte as usize),
                            SyntaxStyle::from_u32(style_id),
                        ));
                    }
                }
            }
        }
    }

    fn trigger_highlighting_for_line(&mut self, line_idx: usize) {
        let line_content = if let Some(buffer) = self.buffers.get_mut(self.active_buffer_index) {
            buffer.highlights.remove(&line_idx);
            buffer.lines.get(line_idx).cloned()
        } else { None };

        if let Some(content) = line_content {
            if !content.is_empty() {
                if let Err(e) = self.plugin_manager.trigger_highlight(line_idx, &content) {
                    self.command_message = format!("Highlight error: {}", e);
                }
            }
        }
    }

    pub fn active_buffer(&mut self) -> Option<&mut Buffer> {
        self.buffers.get_mut(self.active_buffer_index)
    }

    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        self.setup();

        loop {
            if self.should_exit { return Ok(()); }
            self.handle_plugin_events();
            if self.tree_visible { self.update_tree_items(); }
            self.clamp_cursor_position();
            self.update_scroll_offsets(terminal.size()?);
            terminal.draw(|f| ui::ui(self, f))?;
            match self.mode {
                Mode::Insert => execute!(terminal.backend_mut(), SetCursorStyle::BlinkingBar)?,
                _ => execute!(terminal.backend_mut(), SetCursorStyle::BlinkingBlock)?,
            }
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        if self.mode != Mode::Command { self.command_message.clear(); }
                        let mut trigger_highlight = false;

                        if self.tree_view_active && self.tree_visible {
                            self.handle_tree_view_key(key.code);
                        } else {
                            let old_mode = self.mode.clone();
                            let new_mode = match self.mode {
                                Mode::Normal => self.handle_normal_mode_key(key.code),
                                Mode::Insert => {
                                    trigger_highlight = true;
                                    self.handle_insert_mode_key(key.code)
                                },
                                Mode::Command => self.handle_command_mode_key(key.code),
                            };
                            if old_mode == Mode::Insert && new_mode != Mode::Insert {
                                trigger_highlight = true;
                            }
                            self.mode = new_mode;
                        }
                        if trigger_highlight {
                            if let Some(buffer) = self.buffers.get(self.active_buffer_index) {
                                let current_row = buffer.row;
                                self.trigger_highlighting_for_line(current_row);
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn clamp_cursor_position(&mut self) {
        if let Some(buffer) = self.active_buffer() {
            buffer.row = buffer.row.min(buffer.lines.len().saturating_sub(1));
            let grapheme_count = buffer.lines[buffer.row].graphemes(true).count();
            buffer.col = buffer.col.min(grapheme_count);
        }
    }

    fn update_scroll_offsets(&mut self, term_size: Rect) {
        let editor_area = if self.tree_visible {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(self.tree_width),
                    Constraint::Length(1),
                    Constraint::Min(0),
                ])
                .split(term_size);
            chunks[2]
        } else {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0)])
                .split(term_size);
            chunks[0]
        };

        let text_area = {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(2)].as_ref())
                .split(editor_area);
            chunks[0]
        };

        let new_scroll_offset_col = if let Some(buffer) = self.buffers.get(self.active_buffer_index) {
            let line_num_width = buffer.lines.len().to_string().len() + 2;
            let content_width = text_area.width.saturating_sub(line_num_width as u16);
            let pre_cursor_text: String = buffer.lines[buffer.row].graphemes(true).take(buffer.col).collect();
            let pre_cursor_width = UnicodeWidthStr::width(pre_cursor_text.as_str());
            let mut new_offset = self.scroll_offset_col;
            if pre_cursor_width < new_offset {
                new_offset = pre_cursor_width;
            }
            if pre_cursor_width >= new_offset + content_width as usize {
                new_offset = pre_cursor_width - content_width as usize + 1;
            }
            Some(new_offset)
        } else {
            None
        };

        if let Some(buffer) = self.active_buffer() {
            let editor_height = text_area.height;
            if buffer.row < buffer.top_row {
                buffer.top_row = buffer.row;
            }
            if buffer.row >= buffer.top_row + editor_height as usize {
                buffer.top_row = buffer.row - editor_height as usize + 1;
            }
        }

        if let Some(new_offset) = new_scroll_offset_col {
            self.scroll_offset_col = new_offset;
        }
    }

    pub fn mode_str(&self) -> &str {
        match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => "COMMAND",
        }
    }

    pub fn execute_command(&mut self, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() { return; }
        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "q" => {
                if let Some(b) = self.buffers.get(self.active_buffer_index) {
                    if b.modified {
                        self.command_message = "Unsaved changes. Use q! to force quit.".to_string();
                        return;
                    }
                }
                self.should_exit = true;
            }
            "q!" => self.should_exit = true,
            "w" => self.save_file(args.get(0).map(|s| PathBuf::from(s))),
            "wq" => {
                self.save_file(args.get(0).map(|s| PathBuf::from(s)));
                if let Some(b) = self.buffers.get(self.active_buffer_index) {
                    if !b.modified { self.should_exit = true; }
                }
            }
            "e" => {
                if let Some(filename_str) = args.get(0) {
                    self.open_file(PathBuf::from(filename_str));
                } else {
                    self.command_message = "Filename needed for :e".to_string();
                }
            }
            "bn" => {
                if !self.buffers.is_empty() {
                    self.active_buffer_index = (self.active_buffer_index + 1) % self.buffers.len();
                }
            }
            "bp" => {
                if !self.buffers.is_empty() {
                    self.active_buffer_index = (self.active_buffer_index + self.buffers.len() - 1) % self.buffers.len();
                }
            }
            "tt" => {
                self.tree_visible = !self.tree_visible;
                if !self.tree_visible { self.tree_view_active = false; }
            }
            _ => self.command_message = format!("Unknown command: {}", cmd),
        }
    }

    fn open_file_in_new_buffer(&mut self, filename: Option<PathBuf>) {
        let mut new_buffer = Buffer::new(filename.clone());
        let mut message = "Opened new buffer".to_string();

        if let Some(path) = &filename {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        new_buffer.lines = content.lines().map(|s| s.to_string()).collect();
                        if new_buffer.lines.is_empty() {
                            new_buffer.lines.push(String::new());
                        }
                        message = format!("Opened {}", path.display());
                    }
                    Err(e) => message = format!("Error loading {}: {}", path.display(), e),
                }
            } else {
                message = format!("New file: {}", path.display());
            }
        }
        self.buffers.push(new_buffer);
        self.active_buffer_index = self.buffers.len() - 1;
        self.command_message = message;
    }

    pub fn open_file(&mut self, filename: PathBuf) {
        if let Ok(abs_path) = filename.canonicalize() {
            for (i, buffer) in self.buffers.iter().enumerate() {
                if let Some(buf_filename) = &buffer.filename {
                    if let Ok(buf_abs_path) = buf_filename.canonicalize() {
                        if buf_abs_path == abs_path {
                            self.active_buffer_index = i;
                            self.command_message = format!("Switched to buffer {}", abs_path.display());
                            return;
                        }
                    }
                }
            }
        }
        self.open_file_in_new_buffer(Some(filename));
    }

    fn save_file(&mut self, filename: Option<PathBuf>) {
        if let Some(buffer) = self.active_buffer() {
            let target_filename = filename.or_else(|| buffer.filename.clone());
            if let Some(path) = target_filename {
                match std::fs::write(&path, buffer.lines.join("\n")) {
                    Ok(_) => {
                        buffer.filename = Some(path.clone());
                        buffer.modified = false;
                        self.command_message = format!("Saved to {}", path.display());
                    }
                    Err(e) => self.command_message = format!("Error saving {}: {}", path.display(), e),
                }
            } else {
                self.command_message = "No filename. Use :w <filename>".to_string();
            }
        }
    }
    pub fn update_tree_items(&mut self) {
        self.tree_items = tree::get_tree_items(&self.current_path, String::new(), &self.expanded_dirs);
        self.selected_item_index = self.selected_item_index.min(self.tree_items.len().saturating_sub(1));
    }
}
