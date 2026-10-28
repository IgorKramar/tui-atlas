use crate::language::Language;
use crate::process::{expand_home, find_executable};
use serde::Deserialize;
use std::{collections::HashMap, io, path::PathBuf};

#[derive(Clone, Deserialize)]
pub struct Inventory {
    pub id: String,
    pub name: String,
    pub group: String,
    pub version: String,
    pub executable: String,
}

#[derive(Clone, Deserialize)]
pub struct Shortcut {
    pub key: String,
    pub action: String,
    pub context: String,
}

#[derive(Clone, Deserialize)]
pub struct Example {
    pub command: String,
    pub description: String,
}

#[derive(Clone, Deserialize)]
pub struct DocFile {
    pub path: String,
    pub label: String,
}

#[derive(Clone, Deserialize)]
pub struct Source {
    pub title: String,
    pub location: String,
}

#[derive(Clone, Deserialize)]
pub struct Guide {
    pub id: String,
    pub description: String,
    pub launch_args: Vec<String>,
    pub help_args: Option<Vec<String>>,
    pub man_pages: Vec<String>,
    pub doc_files: Vec<DocFile>,
    pub shortcuts: Vec<Shortcut>,
    pub examples: Vec<Example>,
    pub notes: Vec<String>,
    pub sources: Vec<Source>,
}

#[derive(Clone)]
pub struct Program {
    pub language: Language,
    pub info: Inventory,
    pub guide: Guide,
    pub executable: Option<PathBuf>,
    pub search_text: String,
}

impl Program {
    pub fn command_line(&self) -> String {
        std::iter::once(self.info.id.as_str())
            .chain(self.guide.launch_args.iter().map(String::as_str))
            .map(|s| {
                shlex::try_quote(s)
                    .unwrap_or_else(|_| s.into())
                    .into_owned()
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn refresh_presence(&mut self) {
        self.executable = find_executable(&self.info.executable);
    }
}

#[cfg(test)]
pub fn load() -> io::Result<Vec<Program>> {
    load_in(Language::Ru)
}

pub fn load_in(lang: Language) -> io::Result<Vec<Program>> {
    let inventory: Vec<Inventory> = serde_json::from_str(include_str!("../data/inventory.json"))?;
    let mut guides = HashMap::new();
    for source in [
        include_str!("../data/guides-a.json"),
        include_str!("../data/guides-b.json"),
        include_str!("../data/guides-c.json"),
    ] {
        for guide in serde_json::from_str::<Vec<Guide>>(source)? {
            if guides.insert(guide.id.clone(), guide).is_some() {
                return Err(io::Error::other(lang.text(
                    "Повторяющийся идентификатор карточки",
                    "Duplicate guide ID",
                )));
            }
        }
    }
    let mut programs = Vec::new();
    for mut info in inventory {
        let mut guide = guides.remove(&info.id).ok_or_else(|| {
            io::Error::other(crate::tr_format!(
                lang,
                "Нет карточки {}",
                "Missing guide for {}",
                info.id
            ))
        })?;
        let mut search_text = format!("{} {}", info.id, info.executable);
        display_fields(&mut info, &mut guide, |field| {
            search_text.push(' ');
            search_text.push_str(field);
            search_text.push(' ');
            search_text.push_str(english(field));
            if lang == Language::En {
                *field = english(field).into();
            }
        });
        let search_text = search_text.to_lowercase();
        let mut program = Program {
            language: lang,
            info,
            guide,
            executable: None,
            search_text,
        };
        program.refresh_presence();
        programs.push(program);
    }
    if !guides.is_empty() {
        return Err(io::Error::other(lang.text(
            "Карточка отсутствует в инвентаризации",
            "Guide is missing from the inventory",
        )));
    }
    programs.sort_by_key(|p| p.info.name.to_lowercase());
    Ok(programs)
}

fn english(value: &str) -> &str {
    static ENGLISH: std::sync::LazyLock<HashMap<String, String>> = std::sync::LazyLock::new(|| {
        let mut entries = HashMap::new();
        for source in [
            include_str!("../data/catalog-en-a.json"),
            include_str!("../data/catalog-en-b.json"),
            include_str!("../data/catalog-en-c.json"),
        ] {
            for (key, value) in serde_json::from_str::<HashMap<String, String>>(source)
                .expect("embedded catalog translations")
            {
                assert!(
                    entries.insert(key, value).is_none(),
                    "duplicate catalog translation"
                );
            }
        }
        entries
    });
    ENGLISH.get(value).map(String::as_str).unwrap_or(value)
}

fn display_fields(info: &mut Inventory, guide: &mut Guide, mut visit: impl FnMut(&mut String)) {
    visit(&mut info.name);
    visit(&mut info.group);
    visit(&mut guide.description);
    for shortcut in &mut guide.shortcuts {
        visit(&mut shortcut.key);
        visit(&mut shortcut.action);
        visit(&mut shortcut.context);
    }
    for example in &mut guide.examples {
        visit(&mut example.command);
        visit(&mut example.description);
    }
    for note in &mut guide.notes {
        visit(note);
    }
    for doc in &mut guide.doc_files {
        visit(&mut doc.label);
    }
    for source in &mut guide.sources {
        visit(&mut source.title);
    }
}

pub fn personal_sources(program: &Program) -> Vec<DocFile> {
    let lang = program.language;
    let files: &[(&str, &str)] = match program.info.id.as_str() {
        "tmux" => &[(
            "~/.config/tmux/tmux.conf",
            lang.text("Личный конфиг tmux", "Personal tmux configuration"),
        )],
        "yazi" => &[(
            "~/.config/yazi/keymap.toml",
            lang.text("Личные сочетания Yazi", "Personal Yazi shortcuts"),
        )],
        "nchat" => &[(
            "~/.config/nchat/key.conf",
            lang.text("Личные сочетания nchat", "Personal nchat shortcuts"),
        )],
        "nvim" => &[(
            "~/.config/nvim/lua/config/keymaps.lua",
            lang.text("Личные сочетания Neovim", "Personal Neovim shortcuts"),
        )],
        _ => &[],
    };
    files
        .iter()
        .filter(|(path, _)| expand_home(path).is_file())
        .map(|(path, label)| DocFile {
            path: (*path).into(),
            label: (*label).into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_leave_command_identities_and_document_locations_untouched() {
        let russian = load_in(Language::Ru).unwrap();
        let translated_catalogue = load_in(Language::En).unwrap();
        for original in &russian {
            let translated = translated_catalogue
                .iter()
                .find(|p| p.info.id == original.info.id)
                .unwrap();
            assert_eq!(translated.info.executable, original.info.executable);
            assert_eq!(translated.info.version, original.info.version);
            assert_eq!(translated.guide.id, original.guide.id);
            assert_eq!(translated.guide.launch_args, original.guide.launch_args);
            assert_eq!(translated.guide.help_args, original.guide.help_args);
            assert_eq!(translated.guide.man_pages, original.guide.man_pages);
            assert_eq!(translated.command_line(), original.command_line());
            assert_eq!(
                translated
                    .guide
                    .doc_files
                    .iter()
                    .map(|d| &d.path)
                    .collect::<Vec<_>>(),
                original
                    .guide
                    .doc_files
                    .iter()
                    .map(|d| &d.path)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                translated
                    .guide
                    .sources
                    .iter()
                    .map(|d| &d.location)
                    .collect::<Vec<_>>(),
                original
                    .guide
                    .sources
                    .iter()
                    .map(|d| &d.location)
                    .collect::<Vec<_>>()
            );
            let mut fields = original.clone();
            display_fields(&mut fields.info, &mut fields.guide, |field| {
                assert!(translated.search_text.contains(&field.to_lowercase()));
                assert!(
                    original
                        .search_text
                        .contains(&english(field).to_lowercase())
                );
            });
        }
    }

    #[test]
    fn every_curated_catalog_string_has_an_english_translation() {
        for program in load_in(Language::Ru).unwrap() {
            let mut info = program.info;
            let mut guide = program.guide;
            display_fields(&mut info, &mut guide, |field| {
                let russian = |c: char| ('А'..='я').contains(&c) || matches!(c, 'Ё' | 'ё');
                if field.chars().any(russian) {
                    let translated = english(field);
                    assert!(!translated.trim().is_empty());
                    assert!(!translated.chars().any(russian), "Missing English: {field}");
                }
            });
        }
    }

    #[test]
    fn catalogue_has_every_app_and_traced_guides() {
        let catalogue = load().unwrap();
        assert_eq!(catalogue.len(), 37);
        let forge = catalogue
            .iter()
            .find(|p| p.info.id == "forge-deck")
            .unwrap();
        assert_eq!(forge.info.name, "Forge Deck");
        assert_eq!(forge.info.version, "0.3.0");
        assert_eq!(
            forge.guide.help_args.as_deref(),
            Some(["--help".into()].as_slice())
        );
        assert!(
            forge
                .guide
                .shortcuts
                .iter()
                .any(|s| s.key == "G" && s.action.contains("GitHub"))
        );
        assert!(
            forge
                .guide
                .examples
                .iter()
                .any(|e| e.command == "forge-deck --provider github")
        );
        let aerc = catalogue.iter().find(|p| p.info.id == "aerc").unwrap();
        assert_eq!(
            aerc.guide.help_args.as_deref(),
            Some(["--help".into()].as_slice())
        );
        assert!(aerc.guide.man_pages.iter().any(|page| page == "aerc"));
        assert!(
            aerc.guide
                .shortcuts
                .iter()
                .any(|s| s.key == "y" && s.context.contains("отправкой"))
        );
        let glab_tui = catalogue.iter().find(|p| p.info.id == "glab-tui").unwrap();
        assert_eq!(
            glab_tui.guide.help_args.as_deref(),
            Some(["--help".into()].as_slice())
        );
        assert!(
            glab_tui
                .guide
                .shortcuts
                .iter()
                .any(|s| s.key == "D" && s.context.contains("MR/PR"))
        );
        for p in &catalogue {
            assert!(!p.guide.description.is_empty(), "{}", p.info.id);
            assert!(!p.guide.sources.is_empty(), "{}", p.info.id);
            assert!(!p.guide.shortcuts.is_empty(), "{}", p.info.id);
            assert!(!p.guide.examples.is_empty(), "{}", p.info.id);
        }
        let tmux = catalogue.iter().find(|p| p.info.id == "tmux").unwrap();
        assert!(
            tmux.guide
                .shortcuts
                .iter()
                .any(|s| s.key.contains("Space") || s.key.contains("Пробел"))
        );
        let mut missing = tmux.clone();
        missing.info.executable = "/несуществующий-каталог/tmux".into();
        missing.refresh_presence();
        assert!(missing.executable.is_none());
    }
}
