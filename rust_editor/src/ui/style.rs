use ratatui::style::{Color, Modifier, Style};

pub struct UiStyle {
    pub default: Style,
    pub primary_foreground: Color,
    pub primary_background: Color,
    pub highlight_background: Color,
    pub status_bar_background: Color,
    pub status_bar_foreground: Color,
    pub line_number: Style,
    pub tree_selected: Style,
}

impl Default for UiStyle {
    fn default() -> Self {
        Self {
            default: Style::default().fg(Color::White).bg(Color::Black),
            primary_foreground: Color::White,
            primary_background: Color::Black,
            highlight_background: Color::DarkGray,
            status_bar_background: Color::DarkGray,
            status_bar_foreground: Color::White,
            line_number: Style::default().fg(Color::DarkGray),
            tree_selected: Style::default().bg(Color::DarkGray),
        }
    }
}

impl UiStyle {
    pub fn new() -> Self {
        Self::default()
    }
}
