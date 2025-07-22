use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HighlightType {
    Selection,
    SearchResult,
    // 今後、シンタックスハイライトなどを追加できます
}

pub struct UiStyle {
    pub default: Style,
    pub primary_foreground: Color,
    pub primary_background: Color,
    pub highlight_background: Color,
    pub status_bar_background: Color,
    pub status_bar_foreground: Color,
    pub line_number: Style,
    pub tree_selected: Style,
    pub selection_style: Style,
    pub search_result_style: Style,
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
            selection_style: Style::default().bg(Color::Rgb(50, 50, 90)),
            search_result_style: Style::default().bg(Color::Rgb(90, 90, 50)),
        }
    }
}

impl UiStyle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_highlight_style(&self, highlight_type: HighlightType) -> Style {
        match highlight_type {
            HighlightType::Selection => self.selection_style,
            HighlightType::SearchResult => self.search_result_style,
        }
    }
}
