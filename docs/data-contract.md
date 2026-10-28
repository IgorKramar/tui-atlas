# Контракт справочных карточек

Каждый data/guides-*.json — массив объектов. Один объект на программу.

Обязательные поля: id (имя команды, например nvim), description (русское назначение), launch_args (массив стандартных аргументов без имени программы), help_args (массив безопасных аргументов для справки или null), man_pages (массив названий страниц), doc_files (массив объектов path и label), shortcuts (массив объектов key, action, context), examples (массив объектов command и description), notes (массив строк), sources (массив объектов title и location).

Идентификаторы: nvim, yazi, lazygit, lazydocker, k9s, btop, top, systemd-cgtop, dua, nchat, newsboat, browsh, cha, lynx, posting, vd, presenterm, mdv, cliamp, tmux, herdr, nmtui, alsamixer, hyprmoncfg, cfdisk, fzf, tv, atuin, less, more, codex, claude, opencode, cursor-agent, glab-tui, aerc, forge-deck.

Источники — проверенные локальные man/документы/безопасный --help или официальный сайт. Не включать непроверенные комбинации. context обозначает экран/режим/локальное переопределение. Для tmux учесть пользовательский конфиг. Пути в doc_files могут начинаться с ~/; читать только документацию или файлы сочетаний клавиш, без профилей авторизации и рабочих доступов.
