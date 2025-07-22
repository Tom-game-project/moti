use crate::ui::style::HighlightType;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Highlight {
    pub line: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub highlight_type: HighlightType,
}

pub struct Buffer {
    pub filename: Option<PathBuf>,
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub top_row: usize,
    pub modified: bool,
    pub highlights: Vec<Highlight>,
}

impl Buffer {
    pub fn new(filename: Option<PathBuf>) -> Buffer {
        Buffer {
            filename,
            lines: vec![String::new()],
            row: 0,
            col: 0,
            top_row: 0,
            modified: false,
            highlights: Vec::new(),
        }
    }
}
