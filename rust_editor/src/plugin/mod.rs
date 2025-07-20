use crossbeam_channel::{Sender};
use ratatui::style::{Color, Style};

#[derive(Debug)]
pub enum PluginEffect {
    Echo(String),
    GetBufferLineLen {
        line_num: usize,
        sender: Sender<Option<usize>>,
    },
    GetBufferLineData {
        line_num: usize,
        sender: Sender<Option<String>>,
    },
    SetBufferLine {
        line_num: usize,
        text: String,
        sender: Sender<()>
    },
    AddHighlight {
        line: usize,
        start: usize,
        end: usize,
        fg: Option<(u8, u8, u8)>,
        bg: Option<(u8, u8, u8)>,
        sender: Sender<()>
    },
    ClearHighlights(Sender<()>),
}
