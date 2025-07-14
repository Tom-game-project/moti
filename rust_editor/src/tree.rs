use std::collections::HashSet;
use std::path::PathBuf;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Padding, Paragraph},
    Frame,
};

pub struct TreeItem {
    pub path: PathBuf,
    pub prefix: String,
    pub is_dir: bool,
}

pub fn get_tree_items(path: &PathBuf, prefix: String, expanded_dirs: &HashSet<PathBuf>) -> Vec<TreeItem> {
    let mut items = Vec::new();
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
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
            if is_dir && expanded_dirs.contains(&item_path) {
                items.extend(get_tree_items(&item_path, format!("{}  ", prefix), expanded_dirs));
            }
        }
    }
    items
}

pub fn draw_tree_view(
    f: &mut Frame,
    area: Rect,
    tree_items: &[TreeItem],
    tree_scroll_pos: usize,
    selected_item_index: usize,
    expanded_dirs: &HashSet<PathBuf>,
) {
    let tree_block = Block::default()
        .title("ファイル")
        .padding(Padding::horizontal(1));
    let inner_area = tree_block.inner(area);
    let mut lines = Vec::new();

    for (i, item) in tree_items.iter().enumerate().skip(tree_scroll_pos) {
        if i >= tree_scroll_pos + inner_area.height as usize {
            break;
        }
        let indicator = if item.is_dir {
            if expanded_dirs.contains(&item.path) {
                "[-]"
            } else {
                "[+]"
            }
        } else {
            "   "
        };
        let display_text = format!(
            "{}{}{}",
            item.prefix,
            indicator,
            item.path.file_name().unwrap_or_default().to_string_lossy()
        );
        let mut line = Line::from(display_text);
        if i == selected_item_index {
            line = line.style(Style::default().bg(Color::DarkGray));
        }
        lines.push(line);
    }
    let paragraph = Paragraph::new(lines).block(tree_block);
    f.render_widget(paragraph, area);
}
