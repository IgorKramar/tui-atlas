mod app;
mod catalog;
mod docs;
mod language;
mod process;
mod settings;
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

#[derive(Debug, PartialEq, Eq)]
enum CommandLine {
    Run,
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
struct Options {
    language: Option<language::Language>,
    command: CommandLine,
}

fn options(
    args: impl IntoIterator<Item = String>,
    mut lang: language::Language,
) -> io::Result<Options> {
    let mut args = args.into_iter();
    let mut options = Options {
        language: None,
        command: CommandLine::Run,
    };
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--help" | "-h" => options.command = CommandLine::Help,
            "--version" | "-V" => options.command = CommandLine::Version,
            "--lang" => {
                let value = args.next().unwrap_or_default();
                let selected = language::Language::parse(&value).ok_or_else(|| {
                    io::Error::other(crate::tr_format!(
                        lang,
                        "Неверный язык: {value}. Используйте --lang en|ru",
                        "Invalid language: {value}. Use --lang en|ru"
                    ))
                })?;
                options.language = Some(selected);
                lang = selected;
            }
            other => {
                return Err(io::Error::other(crate::tr_format!(
                    lang,
                    "Неизвестный аргумент: {other}. Используйте --help",
                    "Unknown argument: {other}. Use --help"
                )));
            }
        }
    }
    Ok(options)
}

fn help(lang: language::Language) -> &'static str {
    lang.text(
        "TUI Atlas — каталог, читшиты и запуск терминальных приложений\n\nИспользование: tui-atlas [--lang en|ru]\n\n--lang     язык интерфейса; приоритет над сохранённым языком и локалью\n/          поиск приложений\nTab        переключить панель\n1–4        обзор, хоткеи, CLI, документация\nCtrl+f     поиск в тексте\nEnter      запуск с аргументами\nL          сменить и сохранить язык EN/RU (вне ввода)\n?          все клавиши\nq          выход\n\nСправка читается локально. Просмотр карточки не запускает приложение.",
        "TUI Atlas — terminal app catalog, cheatsheets and launcher\n\nUsage: tui-atlas [--lang en|ru]\n\n--lang     interface language; overrides saved language and locale\n/          find apps\nTab        switch pane\n1–4        overview, shortcuts, CLI, documentation\nCtrl+f     find in text\nEnter      launch with arguments\nL          switch and save language EN/RU (outside text input)\n?          all keys\nq          quit\n\nHelp is read locally. Viewing a card does not launch the app."
    )
}

fn save_language(lang: language::Language) -> io::Result<()> {
    settings::save(&settings::path()?, lang)
}

fn settings_error(lang: language::Language, error: &io::Error) -> String {
    crate::tr_format!(
        lang,
        "Настройки недоступны; язык не сохранён: {error}",
        "Settings unavailable; language was not saved: {error}"
    )
}

fn main() -> io::Result<()> {
    let locale = language::Language::locale();
    let options = options(std::env::args().skip(1), locale)?;
    let lang = options.language.unwrap_or(locale);
    match options.command {
        CommandLine::Help => {
            println!("{}", help(lang));
            return Ok(());
        }
        CommandLine::Version => {
            println!("tui-atlas {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        CommandLine::Run => {}
    }
    let saved = settings::path().and_then(|path| settings::load(&path));
    let lang = options
        .language
        .or(saved.as_ref().ok().copied().flatten())
        .unwrap_or(locale);
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(lang.text(
            "Откройте tui-atlas в интерактивном терминале. Справка: tui-atlas --help",
            "Open tui-atlas in an interactive terminal. Help: tui-atlas --help",
        )));
    }
    let mut app = App::new_in(catalog::load_in(lang)?, lang);
    if let Err(error) = saved.and_then(|_| save_language(lang)) {
        app.status = settings_error(lang, &error);
    }
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
                Action::LanguageChanged => {
                    if let Err(error) = save_language(app.language) {
                        app.status = settings_error(app.language, &error);
                    }
                    redraw = true;
                }
                Action::Launch {
                    executable,
                    args,
                    name,
                } => {
                    let result = launch(terminal, &executable, &args, app.language)?;
                    app.status = crate::tr_format!(
                        app.language,
                        "{name}: {result} · вы снова в Atlas",
                        "{name}: {result} · back in Atlas"
                    );
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
    lang: language::Language,
) -> io::Result<String> {
    capture_input(false)?;
    execute!(io::stdout(), LeaveAlternateScreen, Show)?;
    disable_raw_mode()?;
    let result = process::foreground(executable, args);
    let restore = (|| {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        capture_input(true)?;
        terminal.clear()
    })();
    restore.map_err(|error| {
        io::Error::other(crate::tr_format!(
            lang,
            "Не удалось восстановить терминал: {error}",
            "Could not restore the terminal: {error}"
        ))
    })?;
    Ok(match result {
        Ok(status) if status.success() => lang.text("завершено", "finished").into(),
        Ok(status) => crate::tr_format!(lang, "завершено ({status})", "finished ({status})"),
        Err(error) => crate::tr_format!(
            lang,
            "не удалось запустить: {error}",
            "could not launch: {error}"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use language::Language;

    #[test]
    fn cli_validates_all_arguments_and_documents_language() {
        let parse = |values: &[&str]| options(values.iter().map(|s| (*s).to_owned()), Language::En);
        assert_eq!(
            parse(&["--lang", "ru", "--help"]).unwrap(),
            Options {
                language: Some(Language::Ru),
                command: CommandLine::Help
            }
        );
        assert!(parse(&["--lang", "de"]).is_err());
        assert!(parse(&["--lang"]).is_err());
        assert!(parse(&["--help", "unexpected"]).is_err());
        assert!(
            parse(&["--lang", "ru", "unexpected"])
                .unwrap_err()
                .to_string()
                .contains("Неизвестный")
        );
        for lang in [Language::En, Language::Ru] {
            assert!(help(lang).contains("--lang en|ru"));
            assert!(help(lang).contains("EN/RU"));
        }
    }
}
