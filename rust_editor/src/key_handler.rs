use crate::editor::Editor;
use crate::mode::Mode;
use crossterm::event::KeyCode;
use unicode_segmentation::UnicodeSegmentation;

impl Editor {
    pub fn handle_normal_mode_key(&mut self, key_code: KeyCode) -> Mode {
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

    pub fn handle_insert_mode_key(&mut self, key_code: KeyCode) -> Mode {
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

    pub fn handle_command_mode_key(&mut self, key_code: KeyCode) -> Mode {
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

    pub fn handle_tree_view_key(&mut self, key_code: KeyCode) {
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
}
