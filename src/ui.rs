use crate::app::{App, Focus, Mode, Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
};
use unicode_width::UnicodeWidthStr;

const BG: Color = Color::Rgb(14, 18, 27);
const PANEL: Color = Color::Rgb(20, 26, 38);
const BORDER: Color = Color::Rgb(42, 54, 75);
const TEXT: Color = Color::Rgb(218, 227, 241);
const MUTED: Color = Color::Rgb(132, 149, 173);
const CYAN: Color = Color::Rgb(119, 210, 232);
const LILAC: Color = Color::Rgb(186, 168, 246);
const GREEN: Color = Color::Rgb(156, 216, 173);
const PEACH: Color = Color::Rgb(245, 191, 137);

fn block(title: String, active: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(
            title,
            Style::default()
                .fg(if active { CYAN } else { MUTED })
                .bold(),
        ))
        .border_style(Style::default().fg(if active { CYAN } else { BORDER }))
        .style(Style::default().bg(PANEL).fg(TEXT))
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::new().style(Style::default().bg(BG).fg(TEXT)), area);
    if area.width < 36 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(app.language.text(
                "TUI ATLAS\nУвеличьте терминал хотя бы до 36×12.\nq — выход",
                "TUI ATLAS\nResize the terminal to at least 36×12.\nq — quit",
            ))
            .style(Style::default().fg(CYAN))
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(area);
    header(frame, app, chunks[0]);
    search(frame, app, chunks[1]);
    app.hits.tabs.clear();
    app.hits.catalogue = Rect::default();
    app.hits.content = Rect::default();
    if area.width >= 88 {
        let panes = Layout::horizontal([
            Constraint::Length((area.width / 3).clamp(27, 37)),
            Constraint::Min(40),
        ])
        .spacing(1)
        .split(chunks[2]);
        catalogue(frame, app, panes[0]);
        content(frame, app, panes[1]);
    } else if app.focus == Focus::Catalogue {
        catalogue(frame, app, chunks[2]);
    } else {
        content(frame, app, chunks[2]);
    }
    footer(frame, app, chunks[3]);
    match app.mode {
        Mode::Launch => launch_dialog(frame, app),
        Mode::Help => help_dialog(frame, app),
        _ => {}
    }
}

fn header(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::horizontal([
        Constraint::Min(18),
        Constraint::Length(if area.width > 65 { 43 } else { 19 }),
    ])
    .split(area);
    let brand = vec![
        Line::from(vec![
            Span::styled(" ◈ ", Style::default().fg(CYAN)),
            Span::styled("TUI ATLAS", Style::default().fg(TEXT).bold()),
            Span::styled(
                app.language
                    .text("  /  ваш терминальный набор", "  /  your terminal toolkit"),
                Style::default().fg(MUTED),
            ),
        ]),
        Line::styled(
            app.language.text(
                "   Находите. Изучайте. Запускайте.",
                "   Discover. Learn. Launch.",
            ),
            Style::default().fg(MUTED),
        ),
    ];
    frame.render_widget(Paragraph::new(brand), columns[0]);
    let installed = app
        .programs
        .iter()
        .filter(|p| p.executable.is_some())
        .count();
    let keys: usize = app.programs.iter().map(|p| p.guide.shortcuts.len()).sum();
    let text = if area.width > 65 {
        crate::tr_format!(
            app.language,
            "{installed} приложений  ·  {keys} сочетаний  ·  OFFLINE",
            "{installed} apps  ·  {keys} shortcuts  ·  OFFLINE"
        )
    } else {
        crate::tr_format!(app.language, "{installed} приложений", "{installed} apps")
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(text, Style::default().fg(CYAN)),
            Line::styled(
                app.language
                    .text("Каталог · 27 октября 2026", "Catalog · 27 October 2026"),
                Style::default().fg(MUTED),
            ),
        ])
        .right_aligned(),
        columns[1],
    );
}

fn search(frame: &mut Frame, app: &App, area: Rect) {
    let finding = app.mode == Mode::Find
        || (app.focus == Focus::Content && !app.find.is_empty() && app.mode != Mode::Filter);
    let editing = matches!(app.mode, Mode::Find | Mode::Filter);
    let label = if finding {
        app.language
            .text(" ПОИСК В ТЕКСТЕ · Ctrl+f ", " FIND IN TEXT · Ctrl+f ")
    } else {
        app.language
            .text(" ПОИСК ПРИЛОЖЕНИЯ · / ", " FIND AN APP · / ")
    };
    let input = if finding { &app.find } else { &app.filter };
    let placeholder = if finding {
        app.language.text(
            "текст, команда или сочетание…",
            "text, command or shortcut…",
        )
    } else {
        app.language.text(
            "название, категория или действие…",
            "name, category or action…",
        )
    };
    let border = block(label.into(), editing);
    let inner = border.inner(area);
    frame.render_widget(border, area);
    let visible = if input.is_empty() {
        placeholder.to_owned()
    } else {
        tail(input, inner.width.saturating_sub(2) as usize)
    };
    frame.render_widget(
        Paragraph::new(format!(" {visible}")).style(Style::default().fg(if input.is_empty() {
            MUTED
        } else {
            TEXT
        })),
        inner,
    );
    if editing {
        let x = inner.x + 1 + tail(input, inner.width.saturating_sub(2) as usize).width() as u16;
        frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
    }
}

fn catalogue(frame: &mut Frame, app: &mut App, area: Rect) {
    let border = block(
        crate::tr_format!(
            app.language,
            " ПРИЛОЖЕНИЯ · {} / {} ",
            " APPS · {} / {} ",
            app.filtered.len(),
            app.programs.len()
        ),
        app.focus == Focus::Catalogue,
    );
    app.hits.catalogue = border.inner(area);
    let items: Vec<ListItem<'static>> = app
        .filtered
        .iter()
        .map(|&index| {
            let p = &app.programs[index];
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        if p.executable.is_some() {
                            "● "
                        } else {
                            "○ "
                        },
                        Style::default().fg(if p.executable.is_some() { GREEN } else { MUTED }),
                    ),
                    Span::styled(p.info.name.clone(), Style::default().fg(TEXT).bold()),
                ]),
                Line::styled(format!("  {}", p.info.group), Style::default().fg(MUTED)),
            ])
        })
        .collect();
    app.list.select((!items.is_empty()).then_some(app.selected));
    let list = List::new(items)
        .block(border)
        .highlight_symbol("▎")
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(36, 53, 71))
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut app.list);
    if app.filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(app.language.text(
                "\n  Ничего не найдено\n\n  Esc — очистить поиск",
                "\n  Nothing found\n\n  Esc — clear search",
            ))
            .style(Style::default().fg(MUTED)),
            app.hits.catalogue,
        );
    }
}

fn content(frame: &mut Frame, app: &mut App, area: Rect) {
    let (name, version, group, command, present) = app
        .current()
        .map(|p| {
            (
                p.info.name.clone(),
                p.info.version.clone(),
                p.info.group.clone(),
                p.command_line(),
                p.executable.is_some(),
            )
        })
        .unwrap_or_else(|| {
            (
                app.language.text("Пустой выбор", "No selection").into(),
                String::new(),
                String::new(),
                String::new(),
                false,
            )
        });
    let border = block(format!(" {name} "), app.focus == Focus::Content);
    let inner = border.inner(area);
    frame.render_widget(border, area);
    let layout = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    let heading = Line::from(vec![
        Span::styled(format!(" {name} "), Style::default().fg(TEXT).bold()),
        Span::styled(format!("v{version}  "), Style::default().fg(LILAC)),
        Span::styled(
            if present {
                app.language.text("● установлено", "● installed")
            } else {
                app.language.text("○ не найдено", "○ not found")
            },
            Style::default().fg(if present { GREEN } else { PEACH }),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![
            heading,
            Line::styled(
                format!(" {group}  ·  $ {command}"),
                Style::default().fg(MUTED),
            ),
        ]),
        layout[0],
    );
    let mut x = layout[1].x + 1;
    for (i, label) in Tab::labels(app.language).iter().enumerate() {
        let width = (label.width() as u16 + 2).min(layout[1].right().saturating_sub(x));
        if width == 0 {
            break;
        }
        let rect = Rect::new(x, layout[1].y, width, 1);
        let selected = app.tab == Tab::ALL[i];
        let style = if selected {
            Style::default().fg(BG).bg(CYAN).bold()
        } else {
            Style::default().fg(MUTED).bg(PANEL)
        };
        frame.render_widget(Paragraph::new(format!(" {label} ")).style(style), rect);
        app.hits.tabs.push((rect, i));
        x = x.saturating_add(width + 1);
    }
    let body = layout[2];
    let text_area = Rect::new(
        body.x + 1,
        body.y,
        body.width.saturating_sub(3),
        body.height,
    );
    app.hits.content = body;
    app.prepare_view(text_area.width, text_area.height);
    let lines: Vec<Line<'static>> = app
        .view
        .iter()
        .skip(app.scroll)
        .take(text_area.height as usize)
        .map(|line| styled_line(line, &app.find))
        .collect();
    frame.render_widget(Paragraph::new(lines), text_area);
    let mut scrollbar = ScrollbarState::new(app.max_scroll()).position(app.scroll);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(Style::default().fg(BORDER))
            .thumb_style(Style::default().fg(CYAN)),
        body,
        &mut scrollbar,
    );
    let position = if app.max_scroll() == 0 {
        100
    } else {
        app.scroll * 100 / app.max_scroll()
    };
    frame.render_widget(
        Paragraph::new(crate::tr_format!(
            app.language,
            " {} строк · {position}%  │  Ctrl+f — найти  ·  Enter — запуск ",
            " {} lines · {position}%  │  Ctrl+f — find  ·  Enter — launch ",
            app.view.len()
        ))
        .style(Style::default().fg(MUTED))
        .right_aligned(),
        layout[3],
    );
}

fn footer(frame: &mut Frame, app: &App, area: Rect) {
    let shortcuts = match app.mode {
        Mode::Filter | Mode::Find => app.language.text(
            " Enter закончить ввод   Esc назад   Ctrl+u очистить",
            " Enter finish editing   Esc back   Ctrl+u clear",
        ),
        Mode::Launch => app.language.text(
            " Enter запустить   Esc отменить   Ctrl+u очистить аргументы",
            " Enter launch   Esc cancel   Ctrl+u clear arguments",
        ),
        _ => app.language.text(
            " / поиск   Tab панель   1–4 раздел   Enter запуск   L EN/RU   ? помощь   q выход",
            " / find   Tab pane   1–4 section   Enter launch   L EN/RU   ? help   q quit",
        ),
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(shortcuts, Style::default().fg(CYAN)),
            Line::styled(format!(" {}", app.status), Style::default().fg(MUTED)),
        ]),
        area,
    );
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(4));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn launch_dialog(frame: &mut Frame, app: &App) {
    let rect = popup(frame.area(), 82, 11);
    frame.render_widget(Clear, rect);
    let border = block(
        app.language
            .text(" ЗАПУСК ПРИЛОЖЕНИЯ ", " LAUNCH APP ")
            .into(),
        true,
    );
    let inner = border.inner(rect);
    frame.render_widget(border, rect);
    let name = app.current().map(|p| p.info.id.as_str()).unwrap_or("");
    let text = vec![
        Line::styled(format!(" $ {name}"), Style::default().fg(CYAN).bold()),
        Line::styled(
            app.language.text(
                " Аргументы · редактируйте перед запуском",
                " Arguments · edit before launching",
            ),
            Style::default().fg(MUTED),
        ),
        Line::raw(""),
        Line::styled(
            format!(
                " > {}",
                tail(&app.arguments, inner.width.saturating_sub(5) as usize)
            ),
            Style::default().fg(TEXT).bg(BORDER),
        ),
        Line::raw(""),
        Line::styled(
            app.language.text(
                " Кавычки объединяют слова. Shell-выражения не исполняются.",
                " Quotes group words. Shell expressions are not executed.",
            ),
            Style::default().fg(MUTED),
        ),
        Line::styled(
            app.language.text(
                " После выхода вы вернётесь в Atlas.",
                " You will return to Atlas after the program exits.",
            ),
            Style::default().fg(MUTED),
        ),
        Line::styled(
            app.language.text(
                " Enter — запустить                 Esc — отменить",
                " Enter — launch                   Esc — cancel",
            ),
            Style::default().fg(GREEN),
        ),
    ];
    frame.render_widget(Paragraph::new(text), inner);
    let x =
        inner.x + 3 + tail(&app.arguments, inner.width.saturating_sub(5) as usize).width() as u16;
    frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y + 3));
}

fn help_dialog(frame: &mut Frame, app: &App) {
    let rect = popup(frame.area(), 80, 26);
    frame.render_widget(Clear, rect);
    let text = app.language.text("\n  НАВИГАЦИЯ\n  j/k или ↑/↓     выбор приложения / прокрутка текста\n  Tab · h/l        переключение панели\n  /                поиск приложений и действий\n  1 · 2 · 3 · 4    обзор / хоткеи / CLI / документация\n  [ / ]            предыдущий / следующий раздел\n  PgUp/PgDn        прокрутка на страницу\n  gg / G           начало / конец\n  Колесо и клик    работают в обеих панелях\n\n  СПРАВКА И ЗАПУСК\n  Ctrl+f           поиск внутри текущего текста\n  n / N            следующее / предыдущее совпадение\n  r                перечитать локальную справку\n  L                сменить язык: EN / RU\n  Enter или e      запуск с редактируемыми аргументами\n  Esc              закрыть ввод, затем очистить поиск\n  q / Ctrl+c       выход из Atlas\n\n  Enter / Esc / ?  закрыть эту подсказку", "\n  NAVIGATION\n  j/k or ↑/↓      select an app / scroll text\n  Tab · h/l        switch pane\n  /                search apps and actions\n  1 · 2 · 3 · 4    overview / shortcuts / CLI / documentation\n  [ / ]            previous / next section\n  PgUp/PgDn        scroll a page\n  gg / G           top / bottom\n  Wheel and click  work in both panes\n\n  HELP AND LAUNCH\n  Ctrl+f           find in the current text\n  n / N            next / previous match\n  r                reload local help\n  L                switch language: EN / RU\n  Enter or e       launch with editable arguments\n  Esc              close input, then clear search\n  q / Ctrl+c       quit Atlas\n\n  Enter / Esc / ?  close this help");
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(TEXT))
            .wrap(Wrap { trim: false })
            .block(block(
                app.language
                    .text(" УПРАВЛЕНИЕ TUI ATLAS ", " TUI ATLAS CONTROLS ")
                    .into(),
                true,
            )),
        rect,
    );
}

fn tail(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .rev()
        .take_while(|ch| {
            used += unicode_width::UnicodeWidthChar::width(*ch).unwrap_or(0);
            used <= width
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn styled_line(text: &str, query: &str) -> Line<'static> {
    let style = if text.starts_with('▸') || text.starts_with("──") {
        Style::default().fg(PEACH).bold()
    } else if text.starts_with("  $") {
        Style::default().fg(GREEN)
    } else if text.starts_with("http") {
        Style::default().fg(LILAC)
    } else if text.contains("  │  ") {
        Style::default().fg(CYAN)
    } else if text.starts_with("      ") {
        Style::default().fg(MUTED)
    } else {
        Style::default().fg(TEXT)
    };
    if query.is_empty() {
        return Line::styled(text.to_owned(), style);
    }
    let ranges = match_ranges(text, query);
    let mut spans = Vec::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        if start < cursor {
            continue;
        }
        spans.push(Span::styled(text[cursor..start].to_owned(), style));
        spans.push(Span::styled(
            text[start..end].to_owned(),
            Style::default().fg(BG).bg(PEACH).bold(),
        ));
        cursor = end;
    }
    spans.push(Span::styled(text[cursor..].to_owned(), style));
    Line::from(spans)
}

fn match_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut folded = String::new();
    let mut positions = Vec::new();
    for (start, ch) in text.char_indices() {
        let lower: String = ch.to_lowercase().collect();
        positions.extend(std::iter::repeat_n(
            (start, start + ch.len_utf8()),
            lower.len(),
        ));
        folded.push_str(&lower);
    }
    let needle = query.to_lowercase();
    folded
        .match_indices(&needle)
        .map(|(i, s)| (positions[i].0, positions[i + s.len() - 1].1))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn both_languages_render_owned_labels_at_all_sizes_and_help_lists_switch() {
        use crate::language::Language;
        for language in [Language::En, Language::Ru] {
            let mut app = App::new_in(catalog::load_in(language).unwrap(), language);
            for (width, height) in [(130, 40), (54, 18), (25, 8)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                for focus in [Focus::Catalogue, Focus::Content] {
                    app.focus = focus;
                    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                    let screen: String = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|cell| cell.symbol())
                        .collect();
                    assert!(screen.contains("TUI ATLAS"));
                    if width < 36 {
                        assert!(screen.contains(language.text("Увеличьте", "Resize")));
                    } else {
                        assert!(screen.contains(language.text("ПОИСК ПРИЛОЖЕНИЯ", "FIND AN APP")));
                        if width > 100 {
                            assert!(
                                screen.contains("OFFLINE"),
                                "wide header must fit both languages"
                            );
                        }
                    }
                }
                if width > 100 {
                    app.mode = Mode::Help;
                    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                    let screen: String = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|cell| cell.symbol())
                        .collect();
                    assert!(screen.contains("EN / RU"));
                    assert!(screen.contains(language.text("УПРАВЛЕНИЕ", "CONTROLS")));
                    app.mode = Mode::Launch;
                    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
                    let screen: String = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|cell| cell.symbol())
                        .collect();
                    assert!(screen.contains(language.text("ЗАПУСК ПРИЛОЖЕНИЯ", "LAUNCH APP")));
                    app.mode = Mode::Normal;
                }
            }
        }
    }

    #[test]
    fn unicode_search_highlights_valid_original_boundaries() {
        assert_eq!(match_ranges("İ — ПрИвЕт", "привет"), vec![(7, 19)]);
        assert_eq!(match_ranges("İ", "i"), vec![(0, 2)]);
        let _ = styled_line("İ — ПрИвЕт", "i");
    }

    #[test]
    fn renders_wide_narrow_and_tiny_terminals() {
        let mut app = App::new(catalog::load().unwrap());
        for (width, height) in [(130, 40), (54, 18), (25, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(screen.contains("TUI ATLAS"));
            app.focus = Focus::Content;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            app.mode = Mode::Launch;
            terminal.draw(|frame| draw(frame, &mut app)).unwrap();
            app.mode = Mode::Normal;
        }
    }
}
