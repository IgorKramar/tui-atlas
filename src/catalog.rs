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

pub fn load() -> io::Result<Vec<Program>> {
    let inventory: Vec<Inventory> = serde_json::from_str(include_str!("../data/inventory.json"))?;
    let mut guides = HashMap::new();
    for source in [
        include_str!("../data/guides-a.json"),
        include_str!("../data/guides-b.json"),
        include_str!("../data/guides-c.json"),
    ] {
        for guide in serde_json::from_str::<Vec<Guide>>(source)? {
            if guides.insert(guide.id.clone(), guide).is_some() {
                return Err(io::Error::other("Повторяющийся идентификатор карточки"));
            }
        }
    }
    let mut programs = Vec::new();
    for info in inventory {
        let guide = guides
            .remove(&info.id)
            .ok_or_else(|| io::Error::other(format!("Нет карточки {}", info.id)))?;
        let search_text = format!(
            "{} {} {} {} {} {} {}",
            info.name,
            info.id,
            info.group,
            guide.description,
            guide
                .shortcuts
                .iter()
                .map(|s| format!("{} {} {}", s.key, s.action, s.context))
                .collect::<Vec<_>>()
                .join(" "),
            guide
                .examples
                .iter()
                .map(|e| format!("{} {}", e.command, e.description))
                .collect::<Vec<_>>()
                .join(" "),
            guide.notes.join(" ")
        )
        .to_lowercase();
        let mut program = Program {
            info,
            guide,
            executable: None,
            search_text,
        };
        program.refresh_presence();
        programs.push(program);
    }
    if !guides.is_empty() {
        return Err(io::Error::other("Карточка отсутствует в инвентаризации"));
    }
    programs.sort_by_key(|p| p.info.name.to_lowercase());
    Ok(programs)
}

pub fn personal_sources(program: &Program) -> Vec<DocFile> {
    let files: &[(&str, &str)] = match program.info.id.as_str() {
        "tmux" => &[("~/.config/tmux/tmux.conf", "Личный конфиг tmux")],
        "yazi" => &[("~/.config/yazi/keymap.toml", "Личные сочетания Yazi")],
        "nchat" => &[("~/.config/nchat/key.conf", "Личные сочетания nchat")],
        "nvim" => &[(
            "~/.config/nvim/lua/config/keymaps.lua",
            "Личные сочетания Neovim",
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
