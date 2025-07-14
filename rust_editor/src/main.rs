use anyhow::Result;
use crossterm::{
    cursor::SetCursorStyle,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io;

mod buffer;
mod editor;
mod key_handler;
mod mode;
mod plugin;
mod syntax;
mod tree;
mod ui;

use editor::Editor;

fn main() -> Result<()> {
    let mut terminal = {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend)?
    };

    let mut editor = Editor::new()?;
    let res = editor.run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("\nAn error occurred: {:?}\n", err);
    }
    Ok(())
}
