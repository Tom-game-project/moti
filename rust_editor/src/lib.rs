pub mod buffer;
pub mod editor;
pub mod mode;
pub mod plugin;
pub mod tree;
pub mod ui;

use std::io;
use crossterm::{
    cursor::SetCursorStyle,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};

use editor::Editor;

pub async fn run() -> io::Result<()> {
    let mut terminal = {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        Terminal::new(backend)?
    };

    let mut editor = Editor::new().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let res = editor.run(&mut terminal).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }
    Ok(())
}
