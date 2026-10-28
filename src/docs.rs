use crate::{
    catalog::{Program, personal_sources},
    language::Language,
    process::{capture_in, clean_text, expand_home, find_executable},
};
use std::{
    fs::File,
    io::Read,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Cli,
    Manual,
}

pub struct Loaded {
    pub language: Language,
    pub id: String,
    pub kind: Kind,
    pub text: String,
}

pub fn worker() -> (Sender<(Program, Kind)>, Receiver<Loaded>) {
    let (send, requests) = mpsc::channel::<(Program, Kind)>();
    let (responses, receive) = mpsc::channel();
    thread::spawn(move || {
        while let Ok(mut request) = requests.recv() {
            while let Ok(latest) = requests.try_recv() {
                request = latest;
            }
            let (program, kind) = request;
            let text = load(&program, kind);
            if responses
                .send(Loaded {
                    language: program.language,
                    id: program.info.id,
                    kind,
                    text,
                })
                .is_err()
            {
                break;
            }
        }
    });
    (send, receive)
}

fn section(output: &mut String, title: &str, body: &str) {
    output.push_str(&format!("\n── {title} ──\n\n{}\n", body.trim()));
}

fn load(program: &Program, kind: Kind) -> String {
    let lang = program.language;
    let mut output = String::new();
    let started = Instant::now();
    if kind == Kind::Cli {
        match (&program.executable, &program.guide.help_args) {
            (Some(executable), Some(args)) => {
                let command = format!("{} {}", program.info.id, args.join(" "));
                let text = capture_in(executable, args, Duration::from_secs(4), lang)
                    .unwrap_or_else(|e| {
                        crate::tr_format!(
                            lang,
                            "Ошибка чтения справки: {e}",
                            "Could not read help: {e}"
                        )
                    });
                section(&mut output, &command, &text);
            }
            (None, _) => output.push_str(lang.text(
                "Программа не найдена. Доступны встроенные примеры и источники.\n",
                "Program not found. Built-in examples and sources are available.\n",
            )),
            (_, None) => output.push_str(lang.text(
                "Отдельный безопасный --help недоступен; см. раздел «Документация».\n",
                "A separate safe --help is unavailable; see Documentation.\n",
            )),
        }
        return output;
    }
    for source in &program.guide.sources {
        output.push_str(&format!("{}\n{}\n\n", source.title, source.location));
    }
    let mut local_files = personal_sources(program);
    local_files.extend(program.guide.doc_files.clone());
    let mut seen = std::collections::HashSet::new();
    for doc in local_files {
        let path = expand_home(&doc.path);
        if !seen.insert(path.clone()) {
            continue;
        }
        let result = File::open(&path).and_then(|f| {
            let mut bytes = Vec::new();
            const LIMIT: usize = 2 * 1024 * 1024;
            f.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
            let truncated = bytes.len() > LIMIT;
            bytes.truncate(LIMIT);
            let mut text = clean_text(&bytes);
            if truncated {
                text.push_str(lang.text(
                    "\n\n[Файл обрезан: источник больше 2 МиБ]",
                    "\n\n[File truncated: source exceeds 2 MiB]",
                ));
            }
            Ok(text)
        });
        match result {
            Ok(text) => section(&mut output, &format!("{} · {}", doc.label, doc.path), &text),
            Err(error) => section(
                &mut output,
                &doc.label,
                &crate::tr_format!(
                    lang,
                    "Источник недоступен: {error}",
                    "Source unavailable: {error}"
                ),
            ),
        }
    }
    for page in &program.guide.man_pages {
        if started.elapsed() >= Duration::from_secs(12) {
            output.push_str(lang.text(
                "\n[Остальные man-страницы пропущены: превышено время загрузки]\n",
                "\n[Remaining man pages skipped: loading time limit exceeded]\n",
            ));
            break;
        }
        let text = match find_executable("man") {
            Some(man) => capture_in(
                &man,
                &["-P".into(), "cat".into(), page.clone()],
                Duration::from_secs(3),
                lang,
            )
            .unwrap_or_else(|e| {
                crate::tr_format!(
                    lang,
                    "Не удалось открыть man: {e}",
                    "Could not open man: {e}"
                )
            }),
            None => lang
                .text(
                    "Команда man не установлена",
                    "The man command is not installed",
                )
                .into(),
        };
        section(&mut output, &format!("man {page}"), &text);
    }
    if program.guide.man_pages.is_empty() && program.guide.doc_files.is_empty() {
        output.push_str(lang.text("\nПолная локальная документация не найдена. Ссылки выше ведут к первичным источникам; параметры запуска — в разделе «CLI».\n", "\nFull local documentation was not found. The links above point to primary sources; launch options are in CLI.\n"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn owned_document_errors_follow_language_without_changing_external_content() {
        for lang in [Language::En, Language::Ru] {
            let mut program = catalog::load_in(lang)
                .unwrap()
                .into_iter()
                .find(|p| p.info.id == "aerc")
                .unwrap();
            program.executable = None;
            assert!(
                load(&program, Kind::Cli)
                    .contains(lang.text("Программа не найдена", "Program not found"))
            );
            program.executable = Some("/missing-tui-atlas-test/executable".into());
            program.guide.help_args = None;
            assert!(load(&program, Kind::Cli).contains(lang.text(
                "Отдельный безопасный --help недоступен",
                "A separate safe --help is unavailable"
            )));
            program.guide.man_pages.clear();
            program.guide.sources.clear();
            program.guide.doc_files = vec![catalog::DocFile {
                path: "/missing-tui-atlas-test/document".into(),
                label: "Unchanged source title".into(),
            }];
            let text = load(&program, Kind::Manual);
            assert!(text.contains("Unchanged source title"));
            assert!(text.contains(lang.text("Источник недоступен", "Source unavailable")));
        }
    }
}
