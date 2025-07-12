use std::{io, time::{Duration, Instant}, path::PathBuf, collections::HashSet};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};

#[derive(PartialEq, Clone)]
enum Mode {
    Normal,
    Insert,
    Command,
}

struct Buffer {
    filename: Option<PathBuf>,
    lines: Vec<String>,
    row: usize,
    col: usize,
    top_row: usize,
    modified: bool,
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
        }
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
    scroll_offset_row: usize,
    scroll_offset_col: usize,
    should_exit: bool,
    pending_command_prefix: Option<char>,
    
    // Directory Tree Properties
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
    fn new() -> Editor {
        let mut editor = Editor {
            buffers: Vec::new(),
            active_buffer_index: 0,
            mode: Mode::Normal,
            command_input: String::new(),
            scroll_offset_row: 0,
            scroll_offset_col: 0,
            should_exit: false,
            pending_command_prefix: None,
            
            // Directory Tree Properties
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
        editor
    }

    fn active_buffer(&mut self) -> Option<&mut Buffer> {
        self.buffers.get_mut(self.active_buffer_index)
    }

    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        loop {
            if self.should_exit {
                return Ok(());
            }

            // Update tree items before drawing
            if self.tree_visible {
                self.update_tree_items();
            }

            // Apply scroll logic before drawing
            if let Some(buffer) = self.active_buffer() {
                let editor_height = terminal.size()?.height.saturating_sub(2); // Assuming 2 lines for status/command
                // Scroll up if cursor is above the visible area
                if buffer.row < buffer.top_row {
                    buffer.top_row = buffer.row;
                }
                // Scroll down if cursor is below the visible area
                if buffer.row >= buffer.top_row + editor_height as usize {
                    buffer.top_row = buffer.row - editor_height as usize + 1;
                }
            }

            terminal.draw(|f| self.ui(f))?;

            // Manage cursor visibility
            let tree_visible = self.tree_visible;
            let tree_width = self.tree_width;
            let scroll_offset_col = self.scroll_offset_col;

            if self.tree_view_active && tree_visible {
                let _ = terminal.hide_cursor();
            } else if let Some(buffer) = self.active_buffer() {
                let _ = terminal.show_cursor();
                
                // Replicate layout calculation to get the text_buffer_area
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(if tree_visible {
                        vec![Constraint::Length(tree_width), Constraint::Min(0)]
                    } else {
                        vec![Constraint::Min(0)]
                    })
                    .split(terminal.size()?);

                let editor_area_index = if tree_visible { 1 } else { 0 };
                let editor_area = main_chunks[editor_area_index];

                let editor_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
                    .split(editor_area);

                let text_buffer_area = editor_chunks[0];

                let line_num_width = buffer.lines.len().to_string().len() + 2;
                let cursor_x = text_buffer_area.x + 1 + line_num_width as u16 + (buffer.col as u16).saturating_sub(scroll_offset_col as u16);
                let cursor_y = text_buffer_area.y + 1 + (buffer.row as u16).saturating_sub(buffer.top_row as u16);
                let _ = terminal.set_cursor(cursor_x, cursor_y);
            } else {
                let _ = terminal.hide_cursor();
            }

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
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

    fn handle_normal_mode_key(&mut self, key_code: KeyCode) -> Mode {
        let current_mode = self.mode.clone();
        let pending_prefix = self.pending_command_prefix.take();

        if let Some(buffer) = self.active_buffer() {
            if let Some(prefix) = pending_prefix {
                // Handle commands with prefixes (e.g., 'dd')
                match prefix {
                    'd' => {
                        if key_code == KeyCode::Char('d') {
                            // Delete current line
                            if buffer.lines.len() > 1 {
                                buffer.lines.remove(buffer.row);
                                if buffer.row >= buffer.lines.len() {
                                    buffer.row = buffer.lines.len() - 1;
                                }
                            } else {
                                buffer.lines = vec![String::new()];
                                buffer.row = 0;
                            }
                            buffer.col = buffer.col.min(buffer.lines[buffer.row].len());
                            buffer.modified = true;
                        }
                    }
                    _ => {
                        // Unknown prefix, ignore or handle as error
                    }
                }
            } else {
                // Handle single key commands
                match key_code {
                    KeyCode::Char('q') => self.should_exit = true,
                    KeyCode::Char('i') => return Mode::Insert,
                    KeyCode::Char(':') => {
                        self.command_input.clear();
                        return Mode::Command;
                    }
                    KeyCode::Char('h') | KeyCode::Left => {
                        buffer.col = buffer.col.saturating_sub(1);
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        let current_line_len = buffer.lines[buffer.row].len();
                        buffer.col = (buffer.col + 1).min(current_line_len);
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        buffer.row = (buffer.row + 1).min(buffer.lines.len() - 1);
                        buffer.col = buffer.col.min(buffer.lines[buffer.row].len());
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        buffer.row = buffer.row.saturating_sub(1);
                        buffer.col = buffer.col.min(buffer.lines[buffer.row].len());
                    }
                    KeyCode::Char('x') => {
                        if buffer.col < buffer.lines[buffer.row].len() {
                            buffer.lines[buffer.row].remove(buffer.col);
                            buffer.modified = true;
                        }
                    }
                    KeyCode::Char('d') => {
                        self.pending_command_prefix = Some('d');
                    }
                    KeyCode::Char('o') => {
                        buffer.lines.insert(buffer.row + 1, String::new());
                        buffer.row += 1;
                        buffer.col = 0;
                        return Mode::Insert;
                    }
                    KeyCode::Char('O') => {
                        buffer.lines.insert(buffer.row, String::new());
                        buffer.col = 0;
                        return Mode::Insert;
                    }
                    KeyCode::Tab => {
                        if self.tree_visible {
                            self.tree_view_active = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        current_mode
    }

    fn handle_insert_mode_key(&mut self, key_code: KeyCode) -> Mode {
        let current_mode = self.mode.clone();
        if let Some(buffer) = self.active_buffer() {
            match key_code {
                KeyCode::Esc => return Mode::Normal,
                KeyCode::Enter => {
                    let current_line = &mut buffer.lines[buffer.row];
                    let new_line = current_line.split_off(buffer.col);
                    buffer.lines.insert(buffer.row + 1, new_line);
                    buffer.row += 1;
                    buffer.col = 0;
                    buffer.modified = true;
                }
                KeyCode::Backspace => {
                    if buffer.col > 0 {
                        buffer.col -= 1;
                        buffer.lines[buffer.row].remove(buffer.col);
                        buffer.modified = true;
                    } else if buffer.row > 0 {
                        let prev_line = buffer.lines.remove(buffer.row);
                        buffer.row -= 1;
                        buffer.col = buffer.lines[buffer.row].len();
                        buffer.lines[buffer.row].push_str(&prev_line);
                        buffer.modified = true;
                    }
                }
                KeyCode::Left => {
                    buffer.col = buffer.col.saturating_sub(1);
                }
                KeyCode::Right => {
                    let current_line_len = buffer.lines[buffer.row].len();
                    buffer.col = (buffer.col + 1).min(current_line_len);
                }
                KeyCode::Up => {
                    buffer.row = buffer.row.saturating_sub(1);
                    buffer.col = buffer.col.min(buffer.lines[buffer.row].len());
                }
                KeyCode::Down => {
                    buffer.row = (buffer.row + 1).min(buffer.lines.len() - 1);
                    buffer.col = buffer.col.min(buffer.lines[buffer.row].len());
                }
                KeyCode::Char(c) => {
                    buffer.lines[buffer.row].insert(buffer.col, c);
                    buffer.col += 1;
                    buffer.modified = true;
                }
                _ => {}
            }
        }
        current_mode
    }

    fn handle_command_mode_key(&mut self, key_code: KeyCode) -> Mode {
        let current_mode = self.mode.clone();
        match key_code {
            KeyCode::Esc => {
                self.command_input.clear();
                return Mode::Normal;
            }
            KeyCode::Enter => {
                let command = self.command_input.trim().to_string(); // Copy the string
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
        current_mode
    }

    fn handle_tree_view_key(&mut self, key_code: KeyCode) -> Mode {
        let current_mode = self.mode.clone();
        match key_code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected_item_index = (self.selected_item_index + 1).min(self.tree_items.len().saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_item_index = self.selected_item_index.saturating_sub(1);
            }
            KeyCode::Enter => {
                if let Some(selected) = self.tree_items.get(self.selected_item_index) {
                    if selected.is_dir {
                        if self.expanded_dirs.contains(&selected.path) {
                            self.expanded_dirs.remove(&selected.path);
                        } else {
                            self.expanded_dirs.insert(selected.path.clone());
                        }
                        self.update_tree_items();
                    } else {
                        self.open_file(selected.path.clone());
                        self.tree_view_active = false;
                    }
                }
            }
            KeyCode::Char('q') => {
                self.should_exit = true;
            }
            KeyCode::Tab => {
                self.tree_view_active = false;
            }
            _ => {}
        }
        current_mode
    }

    fn get_tree_items(&self, path: &PathBuf, prefix: String) -> Vec<TreeItem> {
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut dirs = Vec::new();
            let mut files = Vec::new();

            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.is_file() {
                    files.push(path);
                }
            }
            dirs.sort();
            files.sort();

            for item_path in dirs.into_iter().chain(files.into_iter()) {
                let is_dir = item_path.is_dir();
                items.push(TreeItem {
                    path: item_path.clone(),
                    prefix: prefix.clone(),
                    is_dir,
                });
                if is_dir && self.expanded_dirs.contains(&item_path) {
                    items.extend(self.get_tree_items(&item_path, format!("{}  ", prefix)));
                }
            }
        } else {
            // Handle permission errors or other read_dir errors
            items.push(TreeItem {
                path: path.join("[Permission Denied]"),
                prefix,
                is_dir: false,
            });
        }
        items
    }

    fn update_tree_items(&mut self) {
        self.tree_items = self.get_tree_items(&self.current_path, String::new());
        // Ensure selected_item_index is within bounds after update
        self.selected_item_index = self.selected_item_index.min(self.tree_items.len().saturating_sub(1));
    }

    fn draw_tree_view(&self, f: &mut Frame, area: ratatui::layout::Rect) {
        let tree_block = Block::default().borders(Borders::ALL).title("Files");
        let inner_area = tree_block.inner(area);

        let mut lines = Vec::new();
        for (i, item) in self.tree_items.iter().enumerate().skip(self.tree_scroll_pos) {
            if i - self.tree_scroll_pos >= inner_area.height as usize { break; }

            let indicator = if item.is_dir {
                if self.expanded_dirs.contains(&item.path) { "-" } else { "+" }
            } else {
                " "
            };
            let display_text = format!("{}{}{} {}", item.prefix, indicator, if item.is_dir { "/" } else { "" }, item.path.file_name().unwrap_or_default().to_string_lossy());
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
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if self.tree_visible {
                vec![Constraint::Length(self.tree_width), Constraint::Min(0)]
            } else {
                vec![Constraint::Min(0)]
            })
            .split(f.size());

        let editor_area_index = if self.tree_visible { 1 } else { 0 };
        let editor_area = main_chunks[editor_area_index];

        if self.tree_visible {
            self.draw_tree_view(f, main_chunks[0]);
            // Draw separator
            let separator_x = main_chunks[0].width;
            for y in 0..f.size().height.saturating_sub(2) { // Exclude status/command lines
                f.buffer_mut().get_mut(separator_x, y).set_symbol("│");
            }
        }

        let editor_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
            .split(editor_area);

        // Text buffer area
        let text_buffer_area = editor_chunks[0];
        let scroll_offset_col = self.scroll_offset_col; // Store in local variable
        if let Some(buffer) = self.active_buffer() {
            let mut buffer_content: Vec<Line> = Vec::new();
            let line_num_width = buffer.lines.len().to_string().len() + 2; // Calculate for rendering

            for (i, line) in buffer.lines.iter().enumerate().skip(buffer.top_row) {
                if i - buffer.top_row >= text_buffer_area.height as usize { break; }

                let line_number_str = format!("{:>width$} ", i + 1, width = line_num_width - 1); // Right-align, add space
                let line_number_span = Span::styled(line_number_str, Style::default().fg(Color::DarkGray)); // Style line numbers

                // Handle horizontal scrolling for the text content
                let display_line_content = if scroll_offset_col < line.len() {
                    &line[scroll_offset_col..]
                } else {
                    ""
                };
                let text_span = Span::raw(display_line_content);

                buffer_content.push(Line::from(vec![line_number_span, text_span]));
            }

            let paragraph = Paragraph::new(buffer_content)
                .block(Block::default().borders(Borders::ALL).title("Editor"))
                .scroll((0, 0)); // Scroll is handled by slicing the string and top_row
            f.render_widget(paragraph, text_buffer_area);
        }

        // Status bar
        let mode_str = match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Command => "COMMAND",
        };

        let status_text = if let Mode::Command = self.mode {
            format!(":{}{}", self.command_input, if Instant::now().duration_since(Instant::now().checked_sub(Duration::from_millis(500)).unwrap_or_else(Instant::now)).as_millis() % 1000 < 500 { "_" } else { " " })
        } else if let Some(buffer) = self.active_buffer() {
            let filename = buffer.filename.as_ref().map_or("[No Name]".to_string(), |p| p.display().to_string());
            let modified_str = if buffer.modified { " [+]" } else { "" };
            format!("{} | {} | Line: {} Col: {}{}", mode_str, filename, buffer.row + 1, buffer.col + 1, modified_str)
        } else {
            format!("{} | No Buffer", mode_str)
        };

        let status_bar = Paragraph::new(status_text)
            .style(Style::default().fg(Color::White).bg(Color::DarkGray))
            .block(Block::default().borders(Borders::NONE));
        f.render_widget(status_bar, editor_chunks[1]);
    }

    fn execute_command(&mut self, command: &str) {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() { return; }

        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "q" => {
                if let Some(buffer) = self.active_buffer() {
                    if buffer.modified {
                        // TODO: Implement proper command buffer message display
                        // self.command_input = format!("Unsaved changes. Use q! to force.");
                        return;
                    }
                }
                self.should_exit = true;
            },
            "q!" => self.should_exit = true,
            "w" => {
                let filename = args.get(0).map(|s| PathBuf::from(s));
                self.save_file(filename);
            },
            "wq" => {
                let filename = args.get(0).map(|s| PathBuf::from(s));
                self.save_file(filename);
                if let Some(buffer) = self.active_buffer() {
                    if !buffer.modified {
                        self.should_exit = true;
                    }
                }
            },
            "e" => {
                if let Some(filename_str) = args.get(0) {
                    self.open_file(PathBuf::from(filename_str));
                }
            },
            "bn" => {
                if !self.buffers.is_empty() {
                    self.active_buffer_index = (self.active_buffer_index + 1) % self.buffers.len();
                }
            },
            "bp" => {
                if !self.buffers.is_empty() {
                    self.active_buffer_index = (self.active_buffer_index + self.buffers.len() - 1) % self.buffers.len();
                }
            },
            "tt" => {
                self.tree_visible = !self.tree_visible;
                if !self.tree_visible {
                    self.tree_view_active = false;
                }
            }
            _ => {
                // TODO: Display unknown command message
            }
        }
    }

    fn open_file_in_new_buffer(&mut self, filename: Option<PathBuf>) {
        let mut new_buffer = Buffer::new(filename.clone());

        if let Some(path) = &filename {
            if path.exists() {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        new_buffer.lines = content.lines().map(|s| s.to_string()).collect();
                        if new_buffer.lines.is_empty() {
                            new_buffer.lines.push(String::new());
                        }
                    },
                    Err(e) => {
                        // TODO: Display error message in command buffer
                        eprintln!("Error loading {}: {}", path.display(), e);
                    }
                }
            }
        }

        self.buffers.push(new_buffer);
        self.active_buffer_index = self.buffers.len() - 1;
        // TODO: Display "Opened X" message in command buffer
    }

    fn open_file(&mut self, filename: PathBuf) {
        let abs_path = filename.canonicalize().unwrap_or(filename.clone());

        for (i, buffer) in self.buffers.iter().enumerate() {
            if let Some(buf_filename) = &buffer.filename {
                if buf_filename.canonicalize().unwrap_or(buf_filename.clone()) == abs_path {
                    self.active_buffer_index = i;
                    return;
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
                        buffer.filename = Some(path);
                        buffer.modified = false;
                        // TODO: Display "Saved to X" message in command buffer
                    },
                    Err(e) => {
                        // TODO: Display error message in command buffer
                        eprintln!("Error saving {}: {}", path.display(), e);
                    }
                }
            }
            else {
                // TODO: Display "No filename. Use :w <filename>" message
            }
        }
    }
}

fn main() -> io::Result<()> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let mut editor = Editor::new();
    let res = editor.run(&mut terminal);

    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}