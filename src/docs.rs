use crate::{
    catalog::{Program, personal_sources},
    process::{capture, clean_text, expand_home, find_executable},
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
    let mut output = String::new();
    let started = Instant::now();
    if kind == Kind::Cli {
        match (&program.executable, &program.guide.help_args) {
            (Some(executable), Some(args)) => {
                let command = format!("{} {}", program.info.id, args.join(" "));
                let text = capture(executable, args, Duration::from_secs(4))
                    .unwrap_or_else(|e| format!("Ошибка чтения справки: {e}"));
                section(&mut output, &command, &text);
            }
            (None, _) => {
                output.push_str("Программа не найдена. Доступны встроенные примеры и источники.\n")
            }
            (_, None) => output
                .push_str("Отдельный безопасный --help недоступен; см. раздел «Документация».\n"),
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
                text.push_str("\n\n[Файл обрезан: источник больше 2 МиБ]");
            }
            Ok(text)
        });
        match result {
            Ok(text) => section(&mut output, &format!("{} · {}", doc.label, doc.path), &text),
            Err(error) => section(
                &mut output,
                &doc.label,
                &format!("Источник недоступен: {error}"),
            ),
        }
    }
    for page in &program.guide.man_pages {
        if started.elapsed() >= Duration::from_secs(12) {
            output.push_str("\n[Остальные man-страницы пропущены: превышено время загрузки]\n");
            break;
        }
        let text = match find_executable("man") {
            Some(man) => capture(
                &man,
                &["-P".into(), "cat".into(), page.clone()],
                Duration::from_secs(3),
            )
            .unwrap_or_else(|e| format!("Не удалось открыть man: {e}")),
            None => "Команда man не установлена".into(),
        };
        section(&mut output, &format!("man {page}"), &text);
    }
    if program.guide.man_pages.is_empty() && program.guide.doc_files.is_empty() {
        output.push_str("\nПолная локальная документация не найдена. Ссылки выше ведут к первичным источникам; параметры запуска — в разделе «CLI».\n");
    }
    output
}
