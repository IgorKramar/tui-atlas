use crate::{
    catalog::{self, Program},
    docs::{self, Kind, Loaded},
    language::Language,
    process::parse_arguments_in,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{layout::Rect, widgets::ListState};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Catalogue,
    Content,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
    Find,
    Launch,
    Help,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Keys,
    Cli,
    Manual,
}
impl Tab {
    pub const ALL: [Self; 4] = [Self::Overview, Self::Keys, Self::Cli, Self::Manual];
    pub fn labels(lang: Language) -> [&'static str; 4] {
        [
            lang.text("1 Обзор", "1 Overview"),
            lang.text("2 Хоткеи", "2 Shortcuts"),
            "3 CLI",
            lang.text("4 Документация", "4 Documentation"),
        ]
    }
    pub fn kind(self) -> Option<Kind> {
        match self {
            Self::Cli => Some(Kind::Cli),
            Self::Manual => Some(Kind::Manual),
            _ => None,
        }
    }
}
pub enum Action {
    None,
    Quit,
    LanguageChanged,
    Launch {
        executable: PathBuf,
        args: Vec<String>,
        name: String,
    },
}

#[derive(Default)]
pub struct HitAreas {
    pub catalogue: Rect,
    pub content: Rect,
    pub tabs: Vec<(Rect, usize)>,
}

pub struct App {
    pub language: Language,
    pub programs: Vec<Program>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub list: ListState,
    pub focus: Focus,
    pub mode: Mode,
    pub tab: Tab,
    pub filter: String,
    pub find: String,
    pub arguments: String,
    pub status: String,
    pub scroll: usize,
    pub view: Vec<String>,
    pub matches: Vec<usize>,
    match_index: Option<usize>,
    pub hits: HitAreas,
    pub page_height: usize,
    view_width: u16,
    dirty: bool,
    g_pending: bool,
    cache: HashMap<(String, Kind), String>,
    requested: Option<(String, Kind)>,
    requests: Sender<(Program, Kind)>,
    responses: Receiver<Loaded>,
}

impl App {
    #[cfg(test)]
    pub fn new(programs: Vec<Program>) -> Self {
        Self::new_in(programs, Language::Ru)
    }

    pub fn new_in(programs: Vec<Program>, lang: Language) -> Self {
        let (requests, responses) = docs::worker();
        let filtered = (0..programs.len()).collect();
        Self {
            language: lang,
            programs,
            filtered,
            selected: 0,
            list: ListState::default(),
            focus: Focus::Catalogue,
            mode: Mode::Normal,
            tab: Tab::Overview,
            filter: String::new(),
            find: String::new(),
            arguments: String::new(),
            status: lang
                .text(
                    "Выберите приложение · Enter — запуск · ? — помощь",
                    "Choose an app · Enter — launch · ? — help",
                )
                .into(),
            scroll: 0,
            view: Vec::new(),
            matches: Vec::new(),
            match_index: None,
            hits: HitAreas::default(),
            page_height: 12,
            view_width: 0,
            dirty: true,
            g_pending: false,
            cache: HashMap::new(),
            requested: None,
            requests,
            responses,
        }
    }

    pub fn current(&self) -> Option<&Program> {
        self.filtered.get(self.selected).map(|&i| &self.programs[i])
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.filtered.get(self.selected).copied()
    }

    pub fn filter_catalogue(&mut self) {
        let previous = self.selected_index();
        let query = self.filter.trim().to_lowercase();
        let terms: Vec<_> = query.split_whitespace().map(str::to_owned).collect();
        self.filtered = self
            .programs
            .iter()
            .enumerate()
            .filter(|(_, p)| terms.iter().all(|term| p.search_text.contains(term)))
            .map(|(i, _)| i)
            .collect();
        self.selected = self
            .filtered
            .iter()
            .position(|&i| {
                self.programs[i].info.id == query
                    || self.programs[i].info.name.to_lowercase() == query
            })
            .or_else(|| previous.and_then(|i| self.filtered.iter().position(|&v| v == i)))
            .unwrap_or(0);
        self.changed_selection();
    }

    fn changed_selection(&mut self) {
        self.scroll = 0;
        self.dirty = true;
        self.list
            .select((!self.filtered.is_empty()).then_some(self.selected));
    }

    pub fn select(&mut self, index: usize) {
        if !self.filtered.is_empty() {
            self.selected = index.min(self.filtered.len() - 1);
            self.changed_selection();
        }
    }

    fn move_selection(&mut self, amount: isize) {
        self.select(self.selected.saturating_add_signed(amount));
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.scroll = 0;
        self.focus = Focus::Content;
        self.dirty = true;
    }

    pub fn poll_docs(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.responses.try_recv() {
            if result.language != self.language {
                continue;
            }
            self.cache.insert((result.id, result.kind), result.text);
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
        changed
    }

    pub fn request_docs(&mut self) {
        let Some(program) = self.current() else {
            return;
        };
        let Some(kind) = self.tab.kind() else {
            return;
        };
        let key = (program.info.id.clone(), kind);
        if !self.cache.contains_key(&key) && self.requested.as_ref() != Some(&key) {
            let _ = self.requests.send((program.clone(), kind));
            self.requested = Some(key);
        }
    }

    pub fn prepare_view(&mut self, width: u16, height: u16) {
        self.page_height = height.max(1) as usize;
        if self.dirty || width != self.view_width {
            self.view = wrap_text(&self.document(), width.max(1) as usize);
            self.view_width = width;
            self.dirty = false;
            self.update_matches();
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }

    fn document(&self) -> String {
        let lang = self.language;
        let Some(p) = self.current() else {
            return lang.text("▸ Ничего не найдено\n\nПопробуйте другое название, категорию или действие.\nEsc — очистить фильтр.", "▸ Nothing found\n\nTry a different name, category or action.\nEsc — clear filter.").into();
        };
        let mut text = String::new();
        match self.tab {
            Tab::Overview => {
                text.push_str(&crate::tr_format!(lang, "▸ НАЗНАЧЕНИЕ\n\n{}\n\n▸ БЫСТРЫЙ ЗАПУСК\n\n  $ {}\n\nEnter откроет запуск с редактируемыми аргументами.\nПосле завершения программы вы вернётесь сюда.\n\n▸ С ЧЕГО НАЧАТЬ\n\n", "▸ PURPOSE\n\n{}\n\n▸ QUICK LAUNCH\n\n  $ {}\n\nEnter opens the launch dialog with editable arguments.\nYou will return here when the program exits.\n\n▸ GETTING STARTED\n\n", p.guide.description, p.command_line()));
                for s in p.guide.shortcuts.iter().take(7) {
                    text.push_str(&format!("  {}  │  {}\n", s.key, s.action));
                }
                text.push_str(lang.text("\n▸ ПОЛЕЗНО ЗНАТЬ\n\n", "\n▸ GOOD TO KNOW\n\n"));
                for note in &p.guide.notes {
                    text.push_str(&format!("• {note}\n\n"));
                }
                text.push_str(lang.text("▸ ИСТОЧНИКИ\n\n", "▸ SOURCES\n\n"));
                for source in &p.guide.sources {
                    text.push_str(&format!("{}\n{}\n\n", source.title, source.location));
                }
            }
            Tab::Keys => {
                text.push_str(lang.text("▸ ГОРЯЧИЕ КЛАВИШИ\n\n", "▸ SHORTCUTS\n\n"));
                for s in &p.guide.shortcuts {
                    text.push_str(&format!("  {}  │  {}\n", s.key, s.action));
                    if !s.context.is_empty() {
                        text.push_str(&format!("      {}\n", s.context));
                    }
                    text.push('\n');
                }
                text.push_str(lang.text("▸ ВЕРСИЯ И НАСТРОЙКИ\n\n", "▸ VERSION AND SETTINGS\n\n"));
                text.push_str(lang.text("Указаны проверенные базовые сочетания. Пометки «локально» относятся к найденным переназначениям. Полные файлы настроек доступны в «Документации». Плагины могут добавлять свои клавиши.\n", "These are verified basic shortcuts. “Local” marks discovered remappings. Full configuration files are available under Documentation. Plugins may add their own shortcuts.\n"));
            }
            Tab::Cli => {
                text.push_str(lang.text("▸ КОМАНДЫ И ПРИМЕРЫ\n\n", "▸ COMMANDS AND EXAMPLES\n\n"));
                for e in &p.guide.examples {
                    text.push_str(&format!("  $ {}\n  {}\n\n", e.command, e.description));
                }
                text.push_str(lang.text("▸ ОРИГИНАЛЬНАЯ CLI-СПРАВКА\n", "▸ ORIGINAL CLI HELP\n"));
                text.push_str(self.source_text(p, Kind::Cli));
            }
            Tab::Manual => {
                text.push_str(lang.text(
                    "▸ ДОКУМЕНТАЦИЯ И ЛИЧНЫЕ НАСТРОЙКИ\n\n",
                    "▸ DOCUMENTATION AND PERSONAL SETTINGS\n\n",
                ));
                text.push_str(self.source_text(p, Kind::Manual));
            }
        }
        text
    }

    fn source_text(&self, program: &Program, kind: Kind) -> &str {
        let lang = self.language;
        self.cache
            .get(&(program.info.id.clone(), kind))
            .map(String::as_str)
            .unwrap_or(lang.text(
                "\nЗагружаю локальный источник… Интерфейс остаётся доступным.\n",
                "\nLoading a local source… The interface remains available.\n",
            ))
    }

    fn update_matches(&mut self) {
        self.match_index = None;
        self.matches = if self.find.is_empty() {
            Vec::new()
        } else {
            let query = self.find.to_lowercase();
            self.view
                .iter()
                .enumerate()
                .filter(|(_, line)| line.to_lowercase().contains(&query))
                .map(|(i, _)| i)
                .collect()
        };
    }

    fn jump_match(&mut self, backwards: bool) {
        let lang = self.language;
        let count = self.matches.len();
        if count == 0 {
            self.status = lang.text("Совпадений: 0", "Matches: 0").into();
            return;
        }
        let index = match self.match_index {
            Some(i) if backwards => (i + count - 1) % count,
            Some(i) => (i + 1) % count,
            None if backwards => self
                .matches
                .iter()
                .rposition(|&row| row < self.scroll)
                .unwrap_or(count - 1),
            None => self
                .matches
                .iter()
                .position(|&row| row >= self.scroll)
                .unwrap_or(0),
        };
        self.match_index = Some(index);
        self.scroll = self.matches[index].min(self.max_scroll());
        self.status = crate::tr_format!(
            lang,
            "Совпадение {}/{} · n/N — следующее/предыдущее",
            "Match {}/{} · n/N — next/previous",
            index + 1,
            count
        );
    }

    fn find_changed(&mut self) {
        self.update_matches();
        self.scroll = 0;
        self.jump_match(false);
    }

    pub fn max_scroll(&self) -> usize {
        self.view.len().saturating_sub(self.page_height)
    }
    pub fn scroll_by(&mut self, amount: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(amount)
            .min(self.max_scroll());
    }

    fn open_launch(&mut self) {
        let lang = self.language;
        match self.current() {
            Some(p) if p.executable.is_some() => {
                self.arguments = p
                    .guide
                    .launch_args
                    .iter()
                    .map(|s| {
                        shlex::try_quote(s)
                            .unwrap_or_else(|_| s.into())
                            .into_owned()
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                self.mode = Mode::Launch;
                self.status = lang
                    .text(
                        "Аргументы передаются программе напрямую; кавычки поддерживаются",
                        "Arguments are passed directly to the program; quotes are supported",
                    )
                    .into();
            }
            Some(_) => {
                self.status = lang
                    .text(
                        "Исполняемый файл не найден. Установите программу отдельно.",
                        "Executable not found. Install the program separately.",
                    )
                    .into()
            }
            None => {
                self.status = lang
                    .text("Сначала выберите приложение", "Select an app first")
                    .into()
            }
        }
    }

    pub fn key(&mut self, key: KeyEvent) -> Action {
        let lang = self.language;
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.mode == Mode::Normal {
                return Action::Quit;
            }
            self.mode = Mode::Normal;
            return Action::None;
        }
        if self.mode == Mode::Help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | '?')
            ) {
                self.mode = Mode::Normal;
            }
            return Action::None;
        }
        if self.mode != Mode::Normal {
            return self.input(key);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            self.mode = Mode::Find;
            self.focus = Focus::Content;
            return Action::None;
        }
        match key.code {
            KeyCode::Char('L') => {
                let next = if self.language == Language::En {
                    Language::Ru
                } else {
                    Language::En
                };
                let id = self.current().map(|p| p.info.id.clone());
                let scroll = self.scroll;
                match catalog::load_in(next) {
                    Ok(programs) => {
                        self.programs = programs;
                        self.language = next;
                        self.filter_catalogue();
                        if let Some(id) = id
                            && let Some(index) = self
                                .filtered
                                .iter()
                                .position(|&i| self.programs[i].info.id == id)
                        {
                            self.select(index);
                        }
                        self.scroll = scroll;
                        self.cache.clear();
                        self.requested = None;
                        self.status = next.text("Язык: русский", "Language: English").into();
                        return Action::LanguageChanged;
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Char('?') | KeyCode::F(1) => self.mode = Mode::Help,
            KeyCode::Char('/') => {
                self.mode = Mode::Filter;
                self.focus = Focus::Catalogue;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Catalogue {
                    Focus::Content
                } else {
                    Focus::Catalogue
                }
            }
            KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::Catalogue,
            KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Content,
            KeyCode::Char('1') => self.set_tab(Tab::Overview),
            KeyCode::Char('2') => self.set_tab(Tab::Keys),
            KeyCode::Char('3') => self.set_tab(Tab::Cli),
            KeyCode::Char('4') => self.set_tab(Tab::Manual),
            KeyCode::Char(']') | KeyCode::Char('[') => {
                let i = Tab::ALL.iter().position(|&t| t == self.tab).unwrap_or(0);
                self.set_tab(
                    Tab::ALL[(i + if key.code == KeyCode::Char(']') { 1 } else { 3 }) % 4],
                );
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.focus == Focus::Catalogue {
                    self.move_selection(1);
                } else {
                    self.scroll_by(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.focus == Focus::Catalogue {
                    self.move_selection(-1);
                } else {
                    self.scroll_by(-1);
                }
            }
            KeyCode::PageDown | KeyCode::Char(' ') => {
                self.scroll_by(self.page_height.saturating_sub(2) as isize)
            }
            KeyCode::PageUp => self.scroll_by(-(self.page_height.saturating_sub(2) as isize)),
            KeyCode::Home => {
                if self.focus == Focus::Catalogue {
                    self.select(0);
                } else {
                    self.scroll = 0;
                }
            }
            KeyCode::End | KeyCode::Char('G') => {
                if self.focus == Focus::Catalogue {
                    self.select(self.filtered.len().saturating_sub(1));
                } else {
                    self.scroll = self.max_scroll();
                }
            }
            KeyCode::Char('g') if self.g_pending => {
                if self.focus == Focus::Catalogue {
                    self.select(0);
                } else {
                    self.scroll = 0;
                }
            }
            KeyCode::Char('g') => {
                self.g_pending = true;
                return Action::None;
            }
            KeyCode::Char('n') => self.jump_match(false),
            KeyCode::Char('N') => self.jump_match(true),
            KeyCode::Enter | KeyCode::Char('e') => self.open_launch(),
            KeyCode::Char('r') => {
                if let Some(i) = self.selected_index() {
                    self.programs[i].refresh_presence();
                    let id = &self.programs[i].info.id;
                    self.cache.retain(|(key, _), _| key != id);
                    self.requested = None;
                    self.dirty = true;
                    self.status = lang
                        .text("Локальная справка обновляется", "Refreshing local help")
                        .into();
                    self.request_docs();
                }
            }
            KeyCode::Esc => {
                if !self.find.is_empty() {
                    self.find.clear();
                    self.update_matches();
                } else {
                    self.filter.clear();
                    self.filter_catalogue();
                }
            }
            _ => {}
        }
        self.g_pending = false;
        Action::None
    }

    fn input(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            return Action::None;
        }
        if key.code == KeyCode::Enter {
            if self.mode == Mode::Launch {
                match parse_arguments_in(&self.arguments, self.language) {
                    Ok(args) => {
                        if let Some(p) = self.current()
                            && let Some(executable) = &p.executable
                        {
                            let action = Action::Launch {
                                executable: executable.clone(),
                                args,
                                name: p.info.name.clone(),
                            };
                            self.mode = Mode::Normal;
                            return action;
                        }
                    }
                    Err(error) => {
                        self.status = error.to_string();
                        return Action::None;
                    }
                }
            }
            self.mode = Mode::Normal;
            return Action::None;
        }
        let target = match self.mode {
            Mode::Filter => &mut self.filter,
            Mode::Find => &mut self.find,
            _ => &mut self.arguments,
        };
        match key.code {
            KeyCode::Backspace => {
                target.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => target.clear(),
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                target.push(ch)
            }
            _ => {}
        }
        match self.mode {
            Mode::Filter => self.filter_catalogue(),
            Mode::Find => {
                self.find_changed();
            }
            _ => {}
        }
        Action::None
    }

    pub fn paste(&mut self, text: &str) {
        let clean: String = text.chars().filter(|ch| !ch.is_control()).collect();
        match self.mode {
            Mode::Filter => {
                self.filter.push_str(&clean);
                self.filter_catalogue();
            }
            Mode::Find => {
                self.find.push_str(&clean);
                self.find_changed();
            }
            Mode::Launch => self.arguments.push_str(&clean),
            _ => {}
        }
    }

    pub fn mouse(&mut self, event: MouseEvent) {
        if self.mode != Mode::Normal {
            return;
        }
        let point = ratatui::layout::Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let delta = if event.kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                };
                if self.hits.catalogue.contains(point) {
                    self.focus = Focus::Catalogue;
                    self.move_selection(delta);
                } else if self.hits.content.contains(point) {
                    self.focus = Focus::Content;
                    self.scroll_by(delta);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, index)) =
                    self.hits.tabs.iter().find(|(rect, _)| rect.contains(point))
                {
                    self.set_tab(Tab::ALL[*index]);
                } else if self.hits.catalogue.contains(point) {
                    let row = (event.row - self.hits.catalogue.y) as usize / 2;
                    self.select(self.list.offset() + row);
                    self.focus = Focus::Catalogue;
                } else if self.hits.content.contains(point) {
                    self.focus = Focus::Content;
                }
            }
            _ => {}
        }
    }
}

pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for source in text.lines() {
        let mut line = String::new();
        let mut used = 0;
        for ch in source.chars() {
            let size = ch.width().unwrap_or(0);
            if used + size > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(ch);
            used += size;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn switching_rebuilds_generated_text_and_rejects_previous_language_worker_results() {
        let mut app = App::new(catalog::load().unwrap());
        app.filter = "tmux".into();
        app.filter_catalogue();
        app.set_tab(Tab::Cli);
        let (responses, receive) = std::sync::mpsc::channel();
        app.responses = receive;
        let (requests, request_receiver) = std::sync::mpsc::channel();
        app.requests = requests;
        app.cache
            .insert(("tmux".into(), Kind::Cli), "Старая справка".into());
        app.prepare_view(90, 15);
        assert!(app.view.iter().any(|line| line.contains("КОМАНДЫ")));
        app.key(KeyEvent::from(KeyCode::Char('L')));
        responses
            .send(Loaded {
                language: Language::Ru,
                id: "tmux".into(),
                kind: Kind::Cli,
                text: "Старый ответ".into(),
            })
            .unwrap();
        assert!(!app.poll_docs());
        app.prepare_view(90, 15);
        assert!(app.view.iter().any(|line| line.contains("COMMANDS")));
        assert!(app.document().contains("Loading a local source"));
        assert!(!app.document().contains("Старый ответ"));
        app.request_docs();
        let (program, kind) = request_receiver.try_recv().unwrap();
        assert_eq!(program.language, Language::En);
        assert_eq!(kind, Kind::Cli);
        responses
            .send(Loaded {
                language: Language::En,
                id: "tmux".into(),
                kind: Kind::Cli,
                text: "external original output".into(),
            })
            .unwrap();
        assert!(app.poll_docs());
        assert!(app.document().contains("external original output"));
        app.key(KeyEvent::from(KeyCode::Char('L')));
        assert_eq!(app.language, Language::Ru);
        assert_eq!(app.current().unwrap().info.id, "tmux");
        assert!(app.document().contains("КОМАНДЫ"));
    }

    #[test]
    fn language_switch_preserves_selection_inputs_and_safe_launch_defaults() {
        use crate::language::Language;
        let mut app = App::new_in(catalog::load_in(Language::Ru).unwrap(), Language::Ru);
        app.filter = "cfdisk".into();
        app.filter_catalogue();
        app.find = "partition".into();
        app.arguments = "--read-only".into();
        app.set_tab(Tab::Keys);
        assert!(matches!(
            app.key(KeyEvent::from(KeyCode::Char('L'))),
            Action::LanguageChanged
        ));
        assert_eq!(app.language, Language::En);
        assert_eq!(app.current().unwrap().info.id, "cfdisk");
        assert_eq!(app.current().unwrap().guide.launch_args, ["--read-only"]);
        assert_eq!(app.filter, "cfdisk");
        assert_eq!(app.find, "partition");
        assert_eq!(app.arguments, "--read-only");
        assert!(app.tab == Tab::Keys && app.focus == Focus::Content);
        assert!(app.document().contains("SHORTCUTS"));
        for mode in [Mode::Filter, Mode::Find, Mode::Launch] {
            app.mode = mode;
            app.key(KeyEvent::from(KeyCode::Char('L')));
            assert_eq!(app.language, Language::En);
        }
        assert!(
            app.filter.ends_with('L') && app.find.ends_with('L') && app.arguments.ends_with('L')
        );
    }

    #[test]
    fn search_is_cyrillic_and_empty_selection_is_safe() {
        let mut app = App::new(catalog::load().unwrap());
        app.filter = "браузер".into();
        app.filter_catalogue();
        assert!(app.filtered.len() >= 3);
        app.filter = "tmux".into();
        app.filter_catalogue();
        assert_eq!(app.current().unwrap().info.id, "tmux");
        app.filter = "Ctrl+@".into();
        app.filter_catalogue();
        assert!(
            app.filtered
                .iter()
                .any(|&i| app.programs[i].info.id == "tmux")
        );
        app.filter = "нет-такого-приложения-абвгд".into();
        app.filter_catalogue();
        assert!(app.current().is_none());
        app.key(KeyEvent::from(KeyCode::Enter));
        app.prepare_view(32, 8);
        assert!(app.view.iter().any(|l| l.contains("Ничего")));
    }

    #[test]
    fn wrapping_preserves_unicode_and_scroll_is_bounded() {
        assert_eq!(wrap_text("Привет 世界", 6).concat(), "Привет 世界");
        let mut app = App::new(catalog::load().unwrap());
        app.set_tab(Tab::Keys);
        app.prepare_view(30, 7);
        app.scroll_by(isize::MAX);
        assert_eq!(app.scroll, app.max_scroll());
        app.scroll_by(isize::MIN);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn search_cycles_matches_at_the_bottom_and_after_paste() {
        let mut app = App::new(catalog::load().unwrap());
        app.view = (0..30)
            .map(|i| {
                if i == 23 || i == 27 {
                    "needle".into()
                } else {
                    "line".into()
                }
            })
            .collect();
        app.page_height = 10;
        app.mode = Mode::Find;
        app.paste("needle");
        assert_eq!(app.scroll, 20);
        assert_eq!(app.match_index, Some(0));
        app.jump_match(false);
        assert_eq!(app.match_index, Some(1));
        app.jump_match(false);
        assert_eq!(app.match_index, Some(0));
        app.jump_match(true);
        assert_eq!(app.match_index, Some(1));
    }

    #[test]
    fn refresh_queues_a_new_request_before_old_results_are_polled() {
        let mut app = App::new(catalog::load().unwrap());
        app.set_tab(Tab::Cli);
        let (send, receive) = std::sync::mpsc::channel();
        app.requests = send;
        let key = (app.current().unwrap().info.id.clone(), Kind::Cli);
        app.cache.insert(key, "старая справка".into());
        app.key(KeyEvent::from(KeyCode::Char('r')));
        assert_eq!(receive.try_recv().unwrap().1, Kind::Cli);
    }
}
