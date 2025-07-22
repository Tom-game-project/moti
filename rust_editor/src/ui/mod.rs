pub mod style;
use crate::editor::Editor;
use ratatui::{
    layout::{Rect, Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
    Frame,
};
use unicode_segmentation::UnicodeSegmentation;

impl Editor {
    pub fn draw_tree_view(&self, f: &mut Frame, area: Rect) {
        let tree_block = Block::default()
            .title("Files")
            .padding(Padding::horizontal(1));
        let inner_area = tree_block.inner(area);
        let mut lines = Vec::new();

        for (i, item) in self.tree_items.iter().enumerate().skip(self.tree_scroll_pos) {
            if i >= self.tree_scroll_pos + inner_area.height as usize { break; }
            let indicator = if item.is_dir { if self.expanded_dirs.contains(&item.path) { "[-u]" } else { "[+]" } } else { "   " };
            let display_text = format!("{}{}{}", item.prefix, indicator, item.path.file_name().unwrap_or_default().to_string_lossy());
            let mut line = Line::from(display_text);
            if i == self.selected_item_index {
                line = line.style(self.style.tree_selected);
            }
            lines.push(line);
        }
        let paragraph = Paragraph::new(lines).block(tree_block);
        f.render_widget(paragraph, area);
    }

    pub fn ui(&mut self, f: &mut Frame) {
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

            for (i, line) in buffer.lines.iter().enumerate().skip(buffer.top_row) {
                if i >= buffer.top_row + text_buffer_area.height as usize { break; }

                let line_number_str = format!("{:>width$}", i + 1, width = line_num_width - 1);
                let line_number_span = Span::styled(format!("{} ", line_number_str), self.style.line_number);

                let mut line_highlights: Vec<&crate::buffer::Highlight> = buffer.highlights.iter()
                    .filter(|h| h.line == i)
                    .collect();
                line_highlights.sort_by_key(|h| h.start_col);

                let mut spans = Vec::new();
                let mut last_col = 0;
                let graphemes: Vec<&str> = line.graphemes(true).collect();

                for highlight in line_highlights {
                    if highlight.start_col > last_col {
                        if let Some(text_slice) = graphemes.get(last_col..highlight.start_col) {
                            let text: String = text_slice.join("");
                            spans.push(Span::raw(text));
                        }
                    }
                    let end_col = highlight.end_col.min(graphemes.len());
                    if let Some(text_slice) = graphemes.get(highlight.start_col..end_col) {
                        let text: String = text_slice.join("");
                        spans.push(Span::styled(text, highlight.style));
                    }
                    last_col = end_col;
                }
                if last_col < graphemes.len() {
                    if let Some(text_slice) = graphemes.get(last_col..) {
                        let text: String = text_slice.join("");
                        spans.push(Span::raw(text));
                    }
                }

                if graphemes.is_empty() {
                    spans.push(Span::raw(""));
                }

                let mut line_spans = vec![line_number_span];
                line_spans.extend(spans);
                buffer_content.push(Line::from(line_spans));
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
        ])).style(Style::default().fg(self.style.status_bar_foreground).bg(self.style.status_bar_background));
        f.render_widget(status_bar, Rect::new(status_area.x, status_area.y, status_area.width, 1));

        let command_line_text = if self.mode == crate::mode::Mode::Command {
            format!(":{}", self.command_input)
        } else {
            self.command_message.clone()
        };
        let command_line = Paragraph::new(command_line_text);
        f.render_widget(command_line, Rect::new(status_area.x, status_area.y + 1, status_area.width, 1));

        if self.mode != crate::mode::Mode::Command && !self.tree_view_active {
            if let Some(buffer) = self.buffers.get(self.active_buffer_index) {
                let line_num_width = buffer.lines.len().to_string().len() + 2;
                let pre_cursor_text: String = buffer.lines[buffer.row].graphemes(true).take(buffer.col).collect();
                let pre_cursor_width = unicode_width::UnicodeWidthStr::width(pre_cursor_text.as_str());
                let cursor_x = text_buffer_area.x + line_num_width as u16 + (pre_cursor_width as u16).saturating_sub(self.scroll_offset_col as u16);
                let cursor_y = text_buffer_area.y + (buffer.row as u16).saturating_sub(buffer.top_row as u16);
                f.set_cursor(cursor_x, cursor_y);
            }
        }
    }
}
