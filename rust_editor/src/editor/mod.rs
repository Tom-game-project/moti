use std::{ collections::HashSet,
    io,
    path::PathBuf,
    time::Duration,
    thread
};
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyCode, KeyEventKind, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
    Frame, Terminal,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use wasmtime::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures::StreamExt;

use crate::buffer::{Buffer, Highlight};
use crate::mode::Mode;
use crate::plugin::PluginEffect;
use crate::tree::TreeItem;

#[derive(Clone)]
pub struct PluginManager {
    engine: Engine,
}

impl PluginManager {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_multi_value(true);
        let engine = Engine::new(&config)?;
        Ok(Self { engine })
    }

    pub fn load_plugin(&self, path: &PathBuf, effect_sender: Sender<PluginEffect>) -> Result<()> {
        let mut store = Store::new(&self.engine, ());
        let mut linker = Linker::new(&self.engine);
        const TIMEOUT: Duration = Duration::from_millis(500);

        let effect_sender_clone = effect_sender.clone();
        linker.func_wrap(
            "host", "echo",
            move |mut caller: Caller<'_, ()>, ptr: i32, len: i32| {
                let mem = match caller.get_export("memory") { Some(Extern::Memory(mem)) => mem, _ => return };
                let mut buffer = vec![0; len as usize];
                if mem.read(&caller, ptr as usize, &mut buffer).is_ok() {
                    if let Ok(message) = String::from_utf8(buffer) {
                        effect_sender_clone.send(PluginEffect::Echo(message)).unwrap();
                    }
                }
            },
        )?;

        let effect_sender_clone = effect_sender.clone();
        linker.func_wrap("host", "get_buffer_line_len", move |line_num: i32| -> i32 {
            let (tx, rx) = unbounded();
            if effect_sender_clone.send(PluginEffect::GetBufferLineLen { line_num: line_num as usize, sender: tx }).is_err() {
                return -1;
            }
            match rx.recv_timeout(TIMEOUT) {
                Ok(Some(len)) => len as i32,
                _ => -1,
            }
        })?;

        let effect_sender_clone = effect_sender.clone();
        linker.func_wrap("host", "get_buffer_line_data", move |mut caller: Caller<'_, ()>, line_num: i32, ptr: i32, len: i32| -> i32 {
            let (tx, rx) = unbounded();
            if effect_sender_clone.send(PluginEffect::GetBufferLineData { line_num: line_num as usize, sender: tx }).is_err() {
                return -1;
            }

            match rx.recv_timeout(TIMEOUT) {
                Ok(Some(line_text)) => {
                    let mem = match caller.get_export("memory") { Some(Extern::Memory(mem)) => mem, _ => return -2 };
                    if line_text.len() as i32 > len { return -3; }
                    if mem.write(&mut caller, ptr as usize, line_text.as_bytes()).is_err() { return -4; }
                    line_text.len() as i32
                },
                _ => -1,
            }
        })?;

        let effect_sender_clone = effect_sender.clone();
        linker.func_wrap("host", "set_buffer_line", move |mut caller: Caller<'_, ()>, line_num: i32, ptr: i32, len: i32| {
            let mem = match caller.get_export("memory") { Some(Extern::Memory(mem)) => mem, _ => return };
            let mut buffer = vec![0; len as usize];
            if mem.read(&caller, ptr as usize, &mut buffer).is_ok() {
                if let Ok(text) = String::from_utf8(buffer) {
                    let (tx, rx) = unbounded();
                    effect_sender_clone.send(PluginEffect::SetBufferLine { line_num: line_num as usize, text, sender: tx }).unwrap();
                    let _ = rx.recv_timeout(TIMEOUT);
                }
            }
        })?;

        let effect_sender_clone = effect_sender.clone();
        linker.func_wrap("host", "add_highlight", move |line: i32, start: i32, end: i32, fg_r: i32, fg_g: i32, fg_b: i32, bg_r: i32, bg_g: i32, bg_b: i32| {
            let fg = if fg_r >= 0 { Some((fg_r as u8, fg_g as u8, fg_b as u8)) } else { None };
            let bg = if bg_r >= 0 { Some((bg_r as u8, bg_g as u8, bg_b as u8)) } else { None };
            let (tx, rx) = unbounded();
            effect_sender_clone.send(PluginEffect::AddHighlight { line: line as usize, start: start as usize, end: end as usize, fg, bg, sender: tx }).unwrap();
            let _ = rx.recv_timeout(TIMEOUT);
        })?;

        let effect_sender_clone = effect_sender.clone();
        linker.func_wrap("host", "clear_highlights", move || {
            let (tx, rx) = unbounded();
            effect_sender_clone.send(PluginEffect::ClearHighlights(tx)).unwrap();
            let _ = rx.recv_timeout(TIMEOUT);
        })?;

        let module = Module::from_file(&self.engine, path)?;
        let instance = linker.instantiate(&mut store, &module)?;
        let init_func = instance.get_typed_func::<(), ()>(&mut store, "init")?;
        init_func.call(&mut store, ())?;

        Ok(())
    }
}

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
    pub fn new() -> Result<Self, wasmtime::Error> {
        let (tx, rx) = unbounded();
        let mut editor = Editor {
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
        editor.expanded_dirs.insert(editor.current_path.clone());
        editor.open_file_in_new_buffer(None);
        editor.command_message.clear();
        Ok(editor)
    }

    pub fn load_plugins(&mut self) {
        let plugin_path = PathBuf::from("./plugin.wasm");
        if !plugin_path.exists() {
            self.command_message = "plugin.wasm not found. Skipping plugin load.".to_string();
            return;
        }

        let plugin_manager = self.plugin_manager.clone();
        let effect_sender = self.plugin_event_sender.clone();
        let error_sender = self.plugin_event_sender.clone();

        thread::spawn(move || {
            if let Err(e) = plugin_manager.load_plugin(&plugin_path, effect_sender) {
                let error_msg = format!("Failed to load plugin: {}", e);
                let _ = error_sender.send(PluginEffect::Echo(error_msg));
            }
        });
    }

pub fn handle_plugin_events(&mut self) {
        while let Ok(effect) = self.plugin_event_receiver.try_recv() {
            match effect {
                PluginEffect::Echo(message) => self.command_message = message,
                PluginEffect::GetBufferLineLen { line_num, sender } => {
                    let len = self.active_buffer().and_then(|b| b.lines.get(line_num).map(|s| s.len()));
                    sender.send(len).unwrap();
                }
                PluginEffect::GetBufferLineData { line_num, sender } => {
                    let line = self.active_buffer().and_then(|b| b.lines.get(line_num).cloned());
                    sender.send(line).unwrap();
                }
                PluginEffect::SetBufferLine { line_num, text, sender } => {
                    if let Some(buffer) = self.active_buffer() {
                        if line_num < buffer.lines.len() {
                            buffer.lines[line_num] = text;
                            buffer.modified = true;
                        }
                    }
                    sender.send(()).unwrap();
                }
                PluginEffect::AddHighlight { line, start, end, fg, bg, sender } => {
                    if let Some(buffer) = self.active_buffer() {
                        let mut style = Style::default();
                        if let Some((r, g, b)) = fg { style = style.fg(Color::Rgb(r, g, b)); }
                        if let Some((r, g, b)) = bg { style = style.bg(Color::Rgb(r, g, b)); }
                        buffer.highlights.push(Highlight { line, start_col: start, end_col: end, style });
                    }
                    sender.send(()).unwrap();
                }
                PluginEffect::ClearHighlights(sender) => {
                    if let Some(buffer) = self.active_buffer() { buffer.highlights.clear(); }
                    sender.send(()).unwrap();
                }
            }
        }
    }

    pub fn active_buffer(&mut self) -> Option<&mut Buffer> {
        self.buffers.get_mut(self.active_buffer_index)
    }

    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        self.load_plugins();
        let mut event_stream = EventStream::new();

        loop {
            if self.should_exit {
                return Ok(());
            }

            self.handle_plugin_events();

            if self.tree_visible {
                self.update_tree_items();
            }
            self.clamp_cursor_position();
            self.update_scroll_offsets(terminal.size()?);

            terminal.draw(|f| self.ui(f))?;

            match self.mode {
                Mode::Insert => {
                    execute!(terminal.backend_mut(), SetCursorStyle::BlinkingBar)?;
                }
                _ => {
                    execute!(terminal.backend_mut(), SetCursorStyle::BlinkingBlock)?;
                }
            }

            if let Some(Ok(Event::Key(key))) = event_stream.next().await {
                if key.kind == KeyEventKind::Press {
                    if self.mode != Mode::Command {
                        self.command_message.clear();
                    }

                    if self.tree_view_active && self.tree_visible {
                        self.handle_tree_view_key(key.code);
                    } else {
                        let new_mode = match self.mode {
                            Mode::Normal => self.handle_normal_mode_key(key.code),
                            Mode::Insert => self.handle_insert_mode_key(key.code),
                            Mode::Command => self.handle_command_mode_key(key.code),
                        };
                        self.mode = new_mode;
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

    fn handle_normal_mode_key(&mut self, key_code: KeyCode) -> Mode {
        let pending_prefix = self.pending_command_prefix.take();

        if let Some(prefix) = pending_prefix {
            if prefix == 'd' && key_code == KeyCode::Char('d') {
                if let Some(buffer) = self.active_buffer() {
                    if buffer.lines.len() > 1 {
                        buffer.lines.remove(buffer.row);
                        if buffer.row >= buffer.lines.len() {
                            buffer.row = buffer.lines.len() - 1;
                        }
                    } else {
                        buffer.lines = vec![String::new()];
                        buffer.row = 0;
                    }
                    buffer.modified = true;
                }
            }
            return Mode::Normal;
        }

        match key_code {
            KeyCode::Char('i') => return Mode::Insert,
            KeyCode::Char(':') => {
                self.command_input.clear();
                self.command_message.clear();
                return Mode::Command;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if let Some(b) = self.active_buffer() { b.col = b.col.saturating_sub(1); }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if let Some(b) = self.active_buffer() { b.col += 1; }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(b) = self.active_buffer() { b.row += 1; }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(b) = self.active_buffer() { b.row = b.row.saturating_sub(1); }
            }
            KeyCode::Char('x') => {
                if let Some(buffer) = self.active_buffer() {
                    let mut graphemes: Vec<&str> = buffer.lines[buffer.row].graphemes(true).collect();
                    if buffer.col < graphemes.len() {
                        graphemes.remove(buffer.col);
                        buffer.lines[buffer.row] = graphemes.join("");
                        buffer.modified = true;
                    }
                }
            }
            KeyCode::Char('d') => self.pending_command_prefix = Some('d'),
            KeyCode::Char('o') => {
                if let Some(b) = self.active_buffer() {
                    b.row += 1;
                    b.lines.insert(b.row, String::new());
                    b.col = 0;
                    b.modified = true;
                }
                return Mode::Insert;
            }
            KeyCode::Char('O') => {
                if let Some(b) = self.active_buffer() {
                    b.lines.insert(b.row, String::new());
                    b.col = 0;
                    b.modified = true;
                }
                return Mode::Insert;
            }
            KeyCode::Tab => {
                if self.tree_visible { self.tree_view_active = true; }
            }
            _ => {}
        }
        Mode::Normal
    }

    fn handle_insert_mode_key(&mut self, key_code: KeyCode) -> Mode {
        if let Some(buffer) = self.active_buffer() {
            buffer.modified = true;
            match key_code {
                KeyCode::Esc => return Mode::Normal,
                KeyCode::Enter => {
                    let line = &mut buffer.lines[buffer.row];
                    let byte_idx = line.grapheme_indices(true).nth(buffer.col).map_or(line.len(), |(i, _)| i);
                    let new_line = line.split_off(byte_idx);
                    buffer.lines.insert(buffer.row + 1, new_line);
                    buffer.row += 1;
                    buffer.col = 0;
                }
                KeyCode::Backspace => {
                    if buffer.col > 0 {
                        let mut graphemes: Vec<&str> = buffer.lines[buffer.row].graphemes(true).collect();
                        buffer.col -= 1;
                        graphemes.remove(buffer.col);
                        buffer.lines[buffer.row] = graphemes.join("");
                    } else if buffer.row > 0 {
                        let prev_line = buffer.lines.remove(buffer.row);
                        buffer.row -= 1;
                        buffer.col = buffer.lines[buffer.row].graphemes(true).count();
                        buffer.lines[buffer.row].push_str(&prev_line);
                    }
                }
                KeyCode::Left => buffer.col = buffer.col.saturating_sub(1),
                KeyCode::Right => buffer.col += 1,
                KeyCode::Up => buffer.row = buffer.row.saturating_sub(1),
                KeyCode::Down => buffer.row += 1,
                KeyCode::Char(c) => {
                    let mut graphemes: Vec<&str> = buffer.lines[buffer.row].graphemes(true).collect();
                    let char_str = c.to_string();
                    graphemes.insert(buffer.col, &char_str);
                    buffer.lines[buffer.row] = graphemes.join("");
                    buffer.col += 1;
                }
                _ => buffer.modified = false,
            }
        }
        Mode::Insert
    }

    fn handle_command_mode_key(&mut self, key_code: KeyCode) -> Mode {
        match key_code {
            KeyCode::Esc => {
                self.command_input.clear();
                self.command_message.clear();
                return Mode::Normal;
            }
            KeyCode::Enter => {
                let command = self.command_input.trim().to_string();
                self.execute_command(&command);
                self.command_input.clear();
                return Mode::Normal;
            }
            KeyCode::Backspace => {
                self.command_input.pop();
            }
            KeyCode::Char(c) => {
                self.command_input.push(c);
            }
            _ => {}
        }
        Mode::Command
    }

    fn handle_tree_view_key(&mut self, key_code: KeyCode) {
        match key_code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected_item_index = (self.selected_item_index + 1).min(self.tree_items.len().saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_item_index = self.selected_item_index.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(selected) = self.tree_items.get(self.selected_item_index) {
                    let path = selected.path.clone();
                    if selected.is_dir {
                        if self.expanded_dirs.contains(&path) {
                            self.expanded_dirs.remove(&path);
                        } else {
                            self.expanded_dirs.insert(path);
                        }
                        self.update_tree_items();
                    } else {
                        self.open_file(path);
                        self.tree_view_active = false;
                    }
                }
            }
            KeyCode::Tab | KeyCode::Esc => {
                self.tree_view_active = false;
            }
            _ => {}
        }
    }

    fn get_tree_items(&self, path: &PathBuf, prefix: String) -> Vec<TreeItem> {
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() { dirs.push(path); } else { files.push(path); }
            }
            dirs.sort();
            files.sort();

            for item_path in dirs.into_iter().chain(files.into_iter()) {
                let is_dir = item_path.is_dir();
                items.push(TreeItem { path: item_path.clone(), prefix: prefix.clone(), is_dir });
                if is_dir && self.expanded_dirs.contains(&item_path) {
                    items.extend(self.get_tree_items(&item_path, format!("{}  ", prefix)));
                }
            }
        }
        items
    }

    fn update_tree_items(&mut self) {
        self.tree_items = self.get_tree_items(&self.current_path, String::new());
        self.selected_item_index = self.selected_item_index.min(self.tree_items.len().saturating_sub(1));
    }

    pub fn mode_str(&self) -> &str {
        match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => "COMMAND",
        }
    }

    fn execute_command(&mut self, command: &str) {
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
            }
            else {
                message = format!("New file: {}", path.display());
            }
        }
        self.buffers.push(new_buffer);
        self.active_buffer_index = self.buffers.len() - 1;
        self.command_message = message;
    }

    fn open_file(&mut self, filename: PathBuf) {
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
            }
        } else {
            self.command_message = "No filename. Use :w <filename>".to_string();
        }
    }
}
