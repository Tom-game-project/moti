use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use crate::syntax::SyntaxStyle;

pub struct Buffer {
    pub filename: Option<PathBuf>,
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub top_row: usize,
    pub modified: bool,
    pub highlights: HashMap<usize, Vec<(Range<usize>, SyntaxStyle)>>,
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
            highlights: HashMap::new(),
        }
    }
    pub fn mark_line_as_modified(&mut self, line_idx: usize) {
        self.highlights.remove(&line_idx);
        self.modified = true;
    }
}
