mod app;
mod catalog;
mod docs;
mod process;
mod ui;

use app::{Action, App};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyEventKind, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use std::{
    io::{self, IsTerminal},
    path::Path,
    time::Duration,
};

fn main() -> io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("tui-atlas {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!(
                "TUI Atlas — каталог, читшиты и запуск терминальных приложений\n\nИспользование: tui-atlas\n\n/          поиск приложений\nTab        переключить панель\n1–4        обзор, хоткеи, CLI, документация\nCtrl+f     поиск в тексте\nEnter      запуск с аргументами\n?          все клавиши\nq          выход\n\nСправка читается локально. Просмотр карточки не запускает приложение."
            );
            return Ok(());
        }
        Some(other) => {
            return Err(io::Error::other(format!(
                "Неизвестный аргумент: {other}. Используйте --help"
            )));
        }
        None => {}
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "Откройте tui-atlas в интерактивном терминале. Справка: tui-atlas --help",
        ));
    }
    let mut app = App::new(catalog::load()?);
    ratatui::run(|terminal| {
        capture_input(true)?;
        let _guard = InputGuard;
        let mut redraw = true;
        loop {
            redraw |= app.poll_docs();
            app.request_docs();
            if redraw {
                terminal.draw(|frame| ui::draw(frame, &mut app))?;
                redraw = false;
            }
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            let action = match event::read()? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    redraw = true;
                    app.key(key)
                }
                Event::Mouse(mouse)
                    if !matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Drag(_)) =>
                {
                    app.mouse(mouse);
                    redraw = true;
                    Action::None
                }
                Event::Resize(_, _) => {
                    redraw = true;
                    Action::None
                }
                Event::Paste(text) => {
                    app.paste(&text);
                    redraw = true;
                    Action::None
                }
                _ => Action::None,
            };
            match action {
                Action::None => {}
                Action::Quit => break Ok(()),
                Action::Launch {
                    executable,
                    args,
                    name,
                } => {
                    let result = launch(terminal, &executable, &args)?;
                    app.status = format!("{name}: {result} · вы снова в Atlas");
                    redraw = true;
                }
            }
        }
    })
}

fn capture_input(enable: bool) -> io::Result<()> {
    if enable {
        execute!(io::stdout(), EnableMouseCapture, EnableBracketedPaste)
    } else {
        execute!(io::stdout(), DisableMouseCapture, DisableBracketedPaste)
    }
}

struct InputGuard;
impl Drop for InputGuard {
    fn drop(&mut self) {
        let _ = capture_input(false);
    }
}

fn launch(
    terminal: &mut ratatui::DefaultTerminal,
    executable: &Path,
    args: &[String],
) -> io::Result<String> {
    capture_input(false)?;
    execute!(io::stdout(), LeaveAlternateScreen, Show)?;
    disable_raw_mode()?;
    let result = process::foreground(executable, args);
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    capture_input(true)?;
    terminal.clear()?;
    Ok(match result {
        Ok(status) if status.success() => "завершено".into(),
        Ok(status) => format!("завершено ({status})"),
        Err(error) => format!("не удалось запустить: {error}"),
    })
}
