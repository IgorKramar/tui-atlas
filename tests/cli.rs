use std::{
    fs,
    process::{Command, Output},
};

#[test]
fn language_priority_and_read_only_cli_commands_are_isolated_from_user_settings() {
    let directory = std::env::temp_dir().join(format!("tui-atlas-cli-{}", std::process::id()));
    let settings = directory.join("tui-atlas/config.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, b"{corrupt").unwrap();
    let run = |args: &[&str], all: &str, messages: &str, language: &str| -> Output {
        Command::new(env!("CARGO_BIN_EXE_tui-atlas"))
            .args(args)
            .env("HOME", &directory)
            .env("XDG_CONFIG_HOME", &directory)
            .env("LC_ALL", all)
            .env("LC_MESSAGES", messages)
            .env("LANG", language)
            .output()
            .unwrap()
    };
    let help = run(&["--help"], "", "ru_RU.UTF-8", "en_US.UTF-8");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Использование"));
    let help = run(&["--help"], "C", "ru_RU.UTF-8", "ru_RU.UTF-8");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage"));
    let help = run(&["--lang", "en", "--help"], "ru_RU.UTF-8", "", "");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("Usage"));
    assert!(run(&["--version"], "C", "", "").status.success());
    assert_eq!(fs::read(&settings).unwrap(), b"{corrupt");
    assert!(!run(&["--lang", "de"], "C", "", "").status.success());
    let unknown = run(&["--lang", "ru", "unexpected"], "C", "", "");
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("Неизвестный аргумент"));

    let original = br#"{"language":"ru","future":true}"#;
    fs::write(&settings, original).unwrap();
    let saved = run(&[], "C", "", "");
    assert!(!saved.status.success());
    assert!(String::from_utf8_lossy(&saved.stderr).contains("интерактивном терминале"));
    let explicit = run(&["--lang", "en"], "ru_RU.UTF-8", "", "");
    assert!(!explicit.status.success());
    assert!(String::from_utf8_lossy(&explicit.stderr).contains("interactive terminal"));
    assert_eq!(fs::read(&settings).unwrap(), original);
    let no_home = Command::new(env!("CARGO_BIN_EXE_tui-atlas"))
        .arg("--help")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .unwrap();
    assert!(no_home.status.success());
    fs::remove_dir_all(directory).unwrap();
}
