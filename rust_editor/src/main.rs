use std::{
    collections::{HashMap, HashSet},
    io,
    ops::Range,
    path::PathBuf,
    time::Duration,
};
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, KeyCode, KeyEventKind},
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

use anyhow::{Context, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use wasmtime::*;

#[derive(PartialEq, Clone, Debug)]
enum Mode {
    Normal,
    Insert,
    Command,
}

#[derive(Debug)]
enum PluginEffect {
    Echo(String),
    ApplyTextStyle {
        line: u32,
        start_byte: u32,
        end_byte: u32,
        style_id: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SyntaxStyle {
    Default = 0,
    Keyword = 1,
    Comment = 2,
    String = 3,
    Number = 4,
    Type = 5,
}

impl SyntaxStyle {
    fn from_u32(id: u32) -> Self {
        match id {
            1 => Self::Keyword,
            2 => Self::Comment,
            3 => Self::String,
            4 => Self::Number,
            5 => Self::Type,
            _ => Self::Default,
        }
    }
}

struct Buffer {
    filename: Option<PathBuf>,
    lines: Vec<String>,
    row: usize,
    col: usize,
    top_row: usize,
    modified: bool,
    highlights: HashMap<usize, Vec<(Range<usize>, SyntaxStyle)>>,
}

impl Buffer {
    fn new(filename: Option<PathBuf>) -> Buffer {
        Buffer {
            filename,
            lines: vec![String::new()],
            row: 0,
            col: 0,
            top_row: 0,
            modified: false,
            highlights: HashMap::new(),
        }
    }
    fn mark_line_as_modified(&mut self, line_idx: usize) {
        self.highlights.remove(&line_idx);
        self.modified = true;
    }
}

struct WasmPlugin {
    instance: Instance,
    store: Store<()>,
    highlight_line_func: Option<TypedFunc<(u32, i32, i32), ()>>,
}

struct PluginManager {
    engine: Engine,
    plugins: Vec<WasmPlugin>,
}

impl PluginManager {
    fn new() -> Result<Self> {
        let engine = Engine::new(&Config::new())?;
        Ok(Self {
            engine,
            plugins: Vec::new(),
        })
    }

    fn load_plugin(&mut self, path: &PathBuf, effect_sender: Sender<PluginEffect>) -> Result<()> {
        let mut store = Store::new(&self.engine, ());
        let mut linker = Linker::new(&self.engine);

        let sender_clone = effect_sender.clone();
        linker.func_wrap(
            "host",
            "apply_text_style",
            move |line: u32, start_byte: u32, end_byte: u32, style_id: u32| {
                sender_clone
                    .send(PluginEffect::ApplyTextStyle {
                        line,
                        start_byte,
                        end_byte,
                        style_id,
                    })
                    .unwrap();
            },
        )?;

        let module = Module::from_file(&self.engine, path)?;
        let instance = linker.instantiate(&mut store, &module)?;

        let highlight_line_func = instance.get_typed_func::<(u32, i32, i32), ()>(&mut store, "highlight_line").ok();

        self.plugins.push(WasmPlugin {
            instance,
            store,
            highlight_line_func,
        });

        Ok(())
    }

    fn trigger_highlight(&mut self, line_idx: usize, line_content: &str) -> Result<()> {
        for plugin in self.plugins.iter_mut() {
            if let Some(func) = &plugin.highlight_line_func {
                let memory = plugin.instance.get_memory(&mut plugin.store, "memory").context("memory export not found")?;
                let ptr = Self::write_string_to_wasm(&mut plugin.store, &plugin.instance, &memory, line_content)?;
                func.call(&mut plugin.store, (line_idx as u32, ptr, line_content.len() as i32))?;
            }
        }
        Ok(())
    }

    fn write_string_to_wasm(store: &mut Store<()>, instance: &Instance, memory: &Memory, s: &str) -> Result<i32> {
        let bytes = s.as_bytes();
        let alloc_func = instance
            .get_typed_func::<i32, i32>(&mut *store, "alloc")
            .context("`alloc` function not found in Wasm module")?;
        
        let ptr = alloc_func.call(&mut *store, bytes.len() as i32)?;
        memory.write(&mut *store, ptr as usize, bytes)?;
        Ok(ptr)
    }
}

struct TreeItem {
    path: PathBuf,
    prefix: String,
    is_dir: bool,
}

struct Editor {
    buffers: Vec<Buffer>,
    active_buffer_index: usize,
    mode: Mode,
    command_input: String,
    command_message: String,
    scroll_offset_col: usize,
    should_exit: bool,
    pending_command_prefix: Option<char>,
    plugin_manager: PluginManager,
    plugin_event_receiver: Receiver<PluginEffect>,
    plugin_event_sender: Sender<PluginEffect>,
    tree_visible: bool,
    tree_view_active: bool,
    tree_width: u16,
    current_path: PathBuf,
    tree_scroll_pos: usize,
    selected_item_index: usize,
    expanded_dirs: HashSet<PathBuf>,
    tree_items: Vec<TreeItem>,
}

impl Editor {
    fn new() -> Result<Self> {
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

    fn setup(&mut self) {
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

    fn active_buffer(&mut self) -> Option<&mut Buffer> {
        self.buffers.get_mut(self.active_buffer_index)
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
        self.setup();

        loop {
            if self.should_exit { return Ok(()); }
            self.handle_plugin_events();
            if self.tree_visible { self.update_tree_items(); }
            self.clamp_cursor_position();
            self.update_scroll_offsets(terminal.size()?);
            terminal.draw(|f| self.ui(f))?;
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
                    buffer.mark_line_as_modified(buffer.row);
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
                        buffer.mark_line_as_modified(buffer.row);
                    }
                }
            }
            KeyCode::Char('d') => self.pending_command_prefix = Some('d'),
            KeyCode::Char('o') => {
                if let Some(b) = self.active_buffer() {
                    b.row += 1;
                    b.lines.insert(b.row, String::new());
                    b.col = 0;
                    b.mark_line_as_modified(b.row);
                }
                return Mode::Insert;
            }
            KeyCode::Char('O') => {
                if let Some(b) = self.active_buffer() {
                    b.lines.insert(b.row, String::new());
                    b.col = 0;
                    b.mark_line_as_modified(b.row);
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
            let current_row = buffer.row;
            buffer.mark_line_as_modified(current_row);

            match key_code {
                KeyCode::Esc => return Mode::Normal,
                KeyCode::Enter => {
                    let line = &mut buffer.lines[current_row];
                    let byte_idx = line.grapheme_indices(true).nth(buffer.col).map_or(line.len(), |(i, _)| i);
                    let new_line = line.split_off(byte_idx);
                    buffer.lines.insert(current_row + 1, new_line);
                    buffer.row += 1;
                    buffer.col = 0;
                }
                KeyCode::Backspace => {
                    if buffer.col > 0 {
                        let mut graphemes: Vec<&str> = buffer.lines[current_row].graphemes(true).collect();
                        buffer.col -= 1;
                        graphemes.remove(buffer.col);
                        buffer.lines[current_row] = graphemes.join("");
                    } else if current_row > 0 {
                        let prev_line = buffer.lines.remove(current_row);
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
                    let mut graphemes: Vec<&str> = buffer.lines[current_row].graphemes(true).collect();
                    let char_str = c.to_string();
                    graphemes.insert(buffer.col, &char_str);
                    buffer.lines[current_row] = graphemes.join("");
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

    fn draw_tree_view(&self, f: &mut Frame, area: Rect) {
        let tree_block = Block::default()
            .title("ファイル")
            .padding(Padding::horizontal(1));
        let inner_area = tree_block.inner(area);
        let mut lines = Vec::new();

        for (i, item) in self.tree_items.iter().enumerate().skip(self.tree_scroll_pos) {
            if i >= self.tree_scroll_pos + inner_area.height as usize { break; }
            let indicator = if item.is_dir { if self.expanded_dirs.contains(&item.path) { "[-]" } else { "[+]" } } else { "   " };
            let display_text = format!("{}{}{}", item.prefix, indicator, item.path.file_name().unwrap_or_default().to_string_lossy());
            let mut line = Line::from(display_text);
            if i == self.selected_item_index {
                line = line.style(Style::default().bg(Color::DarkGray));
            }
            lines.push(line);
        }
        let paragraph = Paragraph::new(lines).block(tree_block);
        f.render_widget(paragraph, area);
    }

    fn ui(&mut self, f: &mut Frame) {
        let main_chunks = if self.tree_visible {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(self.tree_width),
                    Constraint::Length(1),
                    Constraint::Min(0),
                ])
                .split(f.size())
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0)])
                .split(f.size())
        };

        let editor_area = if self.tree_visible { main_chunks[2] } else { main_chunks[0] };

        let editor_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)].as_ref())
            .split(editor_area);

        let text_buffer_area = editor_chunks[0];
        let status_area = editor_chunks[1];

        if self.tree_visible {
            self.draw_tree_view(f, main_chunks[0]);
            let separator_area = main_chunks[1];
            for y in separator_area.y..separator_area.y + separator_area.height.saturating_sub(2) {
                 f.buffer_mut().get_mut(separator_area.x, y).set_symbol("│");
            }
        }

        if let Some(buffer) = self.buffers.get(self.active_buffer_index) {
            let line_num_width = buffer.lines.len().to_string().len() + 2;
            let mut buffer_content: Vec<Line> = Vec::new();

            for (i, line_str) in buffer.lines.iter().enumerate().skip(buffer.top_row) {
                if i >= buffer.top_row + text_buffer_area.height as usize { break; }
                let line_number_str = format!("{:>width$}", i + 1, width = line_num_width - 1);
                let line_number_span = Span::styled(format!("{} ", line_number_str), Style::default().fg(Color::DarkGray));
                
                let mut spans = vec![line_number_span];
                if let Some(highlights) = buffer.highlights.get(&i) {
                    // FIX: More robust rendering logic
                    let mut last_pos = 0;
                    let mut sorted_highlights = highlights.clone();
                    // Sort by start position, then by end position descending (longer ranges first)
                    sorted_highlights.sort_by(|(a, _), (b, _)| {
                        a.start.cmp(&b.start).then(b.end.cmp(&a.end))
                    });

                    for (range, style_id) in &sorted_highlights {
                        // Clamp range to be within line bounds
                        let start = range.start.min(line_str.len());
                        let end = range.end.min(line_str.len());

                        // Skip invalid or already covered ranges
                        if start >= end || start < last_pos {
                            continue;
                        }

                        // Add un-styled text before the current highlight
                        if start > last_pos {
                            spans.push(Span::raw(&line_str[last_pos..start]));
                        }

                        // Add the styled text
                        let style = match style_id {
                            SyntaxStyle::Keyword => Style::default().fg(Color::Magenta),
                            SyntaxStyle::Comment => Style::default().fg(Color::Green),
                            SyntaxStyle::String => Style::default().fg(Color::Yellow),
                            SyntaxStyle::Number => Style::default().fg(Color::Red),
                            SyntaxStyle::Type => Style::default().fg(Color::Cyan),
                            _ => Style::default(),
                        };
                        spans.push(Span::styled(&line_str[start..end], style));

                        // Move position forward
                        last_pos = end;
                    }

                    // Add any remaining un-styled text at the end of the line
                    if last_pos < line_str.len() {
                        spans.push(Span::raw(&line_str[last_pos..]));
                    }
                } else {
                    spans.push(Span::raw(line_str.clone()));
                }
                buffer_content.push(Line::from(spans));
            }

            let paragraph = Paragraph::new(buffer_content)
                .scroll((0, self.scroll_offset_col as u16));
            f.render_widget(paragraph, text_buffer_area);
        }

        let (status_left, status_right) = if let Some(buffer) = self.buffers.get(self.active_buffer_index) {
            let filename = buffer.filename.as_ref().map_or("[No Name]".to_string(), |p| p.display().to_string());
            let modified_str = if buffer.modified { "[+]" } else { "" };
            let left = format!("-- {} -- {} {}", self.mode_str(), filename, modified_str);
            let right = format!("{}:{}", buffer.row + 1, buffer.col + 1);
            (left, right)
        } else {
            (format!("-- {} --", self.mode_str()), String::new())
        };

        let status_bar = Paragraph::new(Line::from(vec![
            Span::raw(&status_left),
            Span::raw(" ".repeat(status_area.width.saturating_sub(status_left.len() as u16 + status_right.len() as u16) as usize)),
            Span::raw(&status_right),
        ])).style(Style::default().fg(Color::White).bg(Color::DarkGray));
        f.render_widget(status_bar, Rect::new(status_area.x, status_area.y, status_area.width, 1));

        let command_line_text = if self.mode == Mode::Command {
            format!(":{}", self.command_input)
        } else {
            self.command_message.clone()
        };
        let command_line = Paragraph::new(command_line_text);
        f.render_widget(command_line, Rect::new(status_area.x, status_area.y + 1, status_area.width, 1));

        if self.mode != Mode::Command && !self.tree_view_active {
            if let Some(buffer) = self.buffers.get(self.active_buffer_index) {
                let line_num_width = buffer.lines.len().to_string().len() + 2;
                let pre_cursor_text: String = buffer.lines[buffer.row].graphemes(true).take(buffer.col).collect();
                let pre_cursor_width = UnicodeWidthStr::width(pre_cursor_text.as_str());
                let cursor_x = text_buffer_area.x + line_num_width as u16 + (pre_cursor_width as u16).saturating_sub(self.scroll_offset_col as u16);
                let cursor_y = text_buffer_area.y + (buffer.row as u16).saturating_sub(buffer.top_row as u16);
                f.set_cursor(cursor_x, cursor_y);
            }
        }
    }

    fn mode_str(&self) -> &str {
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
            } else {
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
            } else {
                self.command_message = "No filename. Use :w <filename>".to_string();
            }
        }
    }
}

fn main() -> Result<()> {
    let mut terminal = {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend)?
    };

    let mut editor = Editor::new()?;
    let res = editor.run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("\nAn error occurred: {:?}\n", err);
    }
    Ok(())
}

