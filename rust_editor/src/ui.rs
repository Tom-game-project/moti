use crate::editor::Editor;
use crate::syntax::SyntaxStyle;
use crate::tree;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;
use unicode_segmentation::UnicodeSegmentation;



pub fn ui(editor: &mut Editor, f: &mut Frame) {
        let main_chunks = if editor.tree_visible {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(editor.tree_width),
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

        let editor_area = if editor.tree_visible { main_chunks[2] } else { main_chunks[0] };

        let editor_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)].as_ref())
            .split(editor_area);

        let text_buffer_area = editor_chunks[0];
        let status_area = editor_chunks[1];

        if editor.tree_visible {
            tree::draw_tree_view(f, main_chunks[0], &editor.tree_items, editor.tree_scroll_pos, editor.selected_item_index, &editor.expanded_dirs);
            let separator_area = main_chunks[1];
            for y in separator_area.y..separator_area.y + separator_area.height.saturating_sub(2) {
                 f.buffer_mut().get_mut(separator_area.x, y).set_symbol("│");
            }
        }

        if let Some(buffer) = editor.buffers.get(editor.active_buffer_index) {
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
                .scroll((0, editor.scroll_offset_col as u16));
            f.render_widget(paragraph, text_buffer_area);
        }

        let (status_left, status_right) = if let Some(buffer) = editor.buffers.get(editor.active_buffer_index) {
            let filename = buffer.filename.as_ref().map_or("[No Name]".to_string(), |p| p.display().to_string());
            let modified_str = if buffer.modified { "[+]" } else { "" };
            let left = format!("-- {} -- {} {}", editor.mode_str(), filename, modified_str);
            let right = format!("{}:{}", buffer.row + 1, buffer.col + 1);
            (left, right)
        } else {
            (format!("-- {} --", editor.mode_str()), String::new())
        };

        let status_bar = Paragraph::new(Line::from(vec![
            Span::raw(&status_left),
            Span::raw(" ".repeat(status_area.width.saturating_sub(status_left.len() as u16 + status_right.len() as u16) as usize)),
            Span::raw(&status_right),
        ])).style(Style::default().fg(Color::White).bg(Color::DarkGray));
        f.render_widget(status_bar, Rect::new(status_area.x, status_area.y, status_area.width, 1));

        let command_line_text = if editor.mode == crate::mode::Mode::Command {
            format!(":{}", editor.command_input)
        } else {
            editor.command_message.clone()
        };
        let command_line = Paragraph::new(command_line_text);
        f.render_widget(command_line, Rect::new(status_area.x, status_area.y + 1, status_area.width, 1));

        if editor.mode != crate::mode::Mode::Command && !editor.tree_view_active {
            if let Some(buffer) = editor.buffers.get(editor.active_buffer_index) {
                let line_num_width = buffer.lines.len().to_string().len() + 2;
                let pre_cursor_text: String = buffer.lines[buffer.row].graphemes(true).take(buffer.col).collect();
                let pre_cursor_width = UnicodeWidthStr::width(pre_cursor_text.as_str());
                let cursor_x = text_buffer_area.x + line_num_width as u16 + (pre_cursor_width as u16).saturating_sub(editor.scroll_offset_col as u16);
                let cursor_y = text_buffer_area.y + (buffer.row as u16).saturating_sub(buffer.top_row as u16);
                f.set_cursor(cursor_x, cursor_y);
            }
        }
    }
