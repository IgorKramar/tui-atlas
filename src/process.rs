use crate::language::Language;
use std::{
    io::{self, Read},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
pub fn parse_arguments(value: &str) -> io::Result<Vec<String>> {
    parse_arguments_in(value, Language::Ru)
}

pub fn parse_arguments_in(value: &str, lang: Language) -> io::Result<Vec<String>> {
    shlex::split(value).ok_or_else(|| {
        io::Error::other(lang.text(
            "Незакрытая кавычка в аргументах",
            "Unclosed quote in arguments",
        ))
    })
}

pub fn clean_text(value: &[u8]) -> String {
    struct Plain(String);
    impl vte::Perform for Plain {
        fn print(&mut self, ch: char) {
            if !ch.is_control() {
                self.0.push(ch);
            }
        }
        fn execute(&mut self, byte: u8) {
            match byte {
                b'\n' => self.0.push('\n'),
                b'\t' => {
                    use unicode_width::UnicodeWidthStr;
                    let width = self.0.rsplit('\n').next().unwrap_or("").width();
                    self.0.extend(std::iter::repeat_n(' ', 8 - width % 8));
                }
                8 if !self.0.ends_with('\n') => {
                    self.0.pop();
                }
                _ => {}
            }
        }
    }
    let mut plain = Plain(String::new());
    vte::Parser::new().advance(&mut plain, value);
    plain.0
}

pub fn expand_home(value: &str) -> PathBuf {
    match value.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(value),
    }
}

pub fn find_executable(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &Path| {
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if name.contains('/') {
        let path = expand_home(name);
        return executable(&path).then_some(path);
    }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join(name))
        .find(|path| executable(path))
}

fn command(executable: &Path, args: &[String]) -> Command {
    if let Some(work_run) = find_executable("work-run") {
        let mut cmd = Command::new(work_run);
        cmd.arg("--").arg(executable).args(args);
        cmd
    } else {
        let mut cmd = Command::new(executable);
        cmd.args(args);
        cmd
    }
}

#[cfg(test)]
pub fn capture(executable: &Path, args: &[String], timeout: Duration) -> io::Result<String> {
    capture_in(executable, args, timeout, Language::Ru)
}

pub fn capture_in(
    executable: &Path,
    args: &[String],
    timeout: Duration,
    lang: Language,
) -> io::Result<String> {
    const LIMIT: u64 = 2 * 1024 * 1024;
    let mut child = command(executable, args)
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0")
        .env("TERM", "dumb")
        .env("PAGER", "cat")
        .env("MANPAGER", "cat")
        .env("MANWIDTH", "105")
        .env("GROFF_NO_SGR", "1")
        .env("LC_ALL", "C.UTF-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let read = |stream: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.take(LIMIT).read_to_end(&mut bytes);
            bytes
        })
    };
    let out = read(Box::new(child.stdout.take().expect("stdout pipe")));
    let err = read(Box::new(child.stderr.take().expect("stderr pipe")));
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            // Закрываем и потомков: иначе они могут удерживать stdout после таймаута.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    let mut text = clean_text(&stdout);
    let errors = clean_text(&stderr);
    if !errors.trim().is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(errors.trim());
    }
    if timed_out {
        text.push_str(lang.text(
            "\n\n[Источник превысил время ожидания и остановлен]",
            "\n\n[Source timed out and was stopped]",
        ));
    }
    if stdout.len() as u64 >= LIMIT || stderr.len() as u64 >= LIMIT {
        text.push_str(lang.text(
            "\n\n[Справка обрезана: источник больше 2 МиБ]",
            "\n\n[Help truncated: source exceeds 2 MiB]",
        ));
    }
    if !status.success() && text.trim().is_empty() {
        text = crate::tr_format!(
            lang,
            "Источник завершился с ошибкой: {status}",
            "Source exited with an error: {status}"
        );
    }
    Ok(text)
}

pub fn foreground(executable: &Path, args: &[String]) -> io::Result<ExitStatus> {
    // Ctrl+C должен завершать дочернюю программу, сохраняя сам справочник.
    let previous = unsafe { libc::signal(libc::SIGINT, libc::SIG_IGN) };
    let mut cmd = command(executable, args);
    unsafe {
        cmd.pre_exec(|| {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            Ok(())
        });
    }
    let result = cmd.status();
    unsafe {
        libc::signal(libc::SIGINT, previous);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_errors_do_not_translate_external_output_or_shell_arguments() {
        assert!(
            parse_arguments_in("'", Language::En)
                .unwrap_err()
                .to_string()
                .contains("Unclosed quote")
        );
        let output = capture_in(
            Path::new("/bin/sh"),
            &["-c".into(), "printf 'Оригинальный текст'".into()],
            Duration::from_secs(2),
            Language::En,
        )
        .unwrap();
        assert_eq!(output, "Оригинальный текст");
        let output = capture_in(
            Path::new("/bin/sh"),
            &["-c".into(), "sleep 8 & wait".into()],
            Duration::from_millis(150),
            Language::En,
        )
        .unwrap();
        assert!(output.contains("Source timed out and was stopped"));
    }

    #[test]
    fn arguments_preserve_quotes_and_do_not_interpret_shell() {
        assert_eq!(
            parse_arguments(r#"--file "два слова.md" ';' '$HOME'"#).unwrap(),
            vec!["--file", "два слова.md", ";", "$HOME"]
        );
        assert!(parse_arguments("'незакрытая кавычка").is_err());
    }

    #[test]
    fn external_text_cannot_emit_terminal_controls() {
        assert_eq!(
            clean_text(b"\x1b[31mred\x1b[0m\x1b]52;c;c2VjcmV0\x07! a\x08b\x00\n"),
            "red! b\n"
        );
    }

    #[test]
    fn timeout_also_stops_children_holding_the_output_pipe() {
        let start = Instant::now();
        let output = capture(
            Path::new("/bin/sh"),
            &["-c".into(), "sleep 8 & wait".into()],
            Duration::from_millis(150),
        )
        .unwrap();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(output.contains("время ожидания"));
    }
}
