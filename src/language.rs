#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    Ru,
}

impl Language {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "en" => Some(Self::En),
            "ru" => Some(Self::Ru),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ru => "ru",
        }
    }

    pub fn text<'a>(self, ru: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::Ru => ru,
        }
    }

    pub fn selected(explicit: Option<Self>, saved: Option<Self>, locales: &[String]) -> Self {
        explicit.or(saved).unwrap_or_else(|| {
            if locales.iter().find(|s| !s.is_empty()).is_some_and(|s| {
                s.split(['_', '-', '.', '@'])
                    .next()
                    .is_some_and(|s| s.eq_ignore_ascii_case("ru"))
            }) {
                Self::Ru
            } else {
                Self::En
            }
        })
    }

    pub fn locale() -> Self {
        Self::selected(
            None,
            None,
            &["LC_ALL", "LC_MESSAGES", "LANG"].map(|key| std::env::var(key).unwrap_or_default()),
        )
    }
}

#[macro_export]
macro_rules! tr_format {
    ($lang:expr, $ru:literal, $en:literal $(, $args:expr)* $(,)?) => {
        match $lang {
            $crate::language::Language::Ru => format!($ru $(, $args)*),
            $crate::language::Language::En => format!($en $(, $args)*),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::Language::*;
    use super::*;

    #[test]
    fn explicit_then_saved_then_first_nonempty_locale_then_english() {
        let locales = |values: &[&str]| values.iter().map(|s| (*s).into()).collect::<Vec<String>>();
        assert_eq!(
            Language::selected(Some(En), Some(Ru), &locales(&["ru_RU.UTF-8"])),
            En
        );
        assert_eq!(
            Language::selected(None, Some(Ru), &locales(&["en_US.UTF-8"])),
            Ru
        );
        assert_eq!(
            Language::selected(None, None, &locales(&["", "ru_RU.UTF-8", "en"])),
            Ru
        );
        assert_eq!(
            Language::selected(None, None, &locales(&["C", "ru_RU.UTF-8"])),
            En
        );
        assert_eq!(Language::selected(None, None, &[]), En);
        assert_eq!(Language::parse("de"), None);
    }
}
