use sissy::i18n::Language;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}
impl ThemeChoice {
    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preferences {
    pub language: Language,
    pub theme: ThemeChoice,
    pub scale: f32,
    pub large: u64,
    pub days: u64,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::Portuguese,
            theme: ThemeChoice::System,
            scale: 1.,
            large: 500,
            days: 90,
        }
    }
}
impl Preferences {
    fn folder() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Application Support/Sissy"))
    }
    pub fn load() -> Self {
        let Some(folder) = Self::folder() else {
            return Self::default();
        };
        if let Ok(contents) = std::fs::read_to_string(folder.join("preferences")) {
            return Self::parse(&contents);
        }
        Self {
            language: std::fs::read_to_string(folder.join("language"))
                .map(|code| Language::from_code(&code))
                .unwrap_or_default(),
            ..Self::default()
        }
    }
    fn parse(contents: &str) -> Self {
        let mut prefs = Self::default();
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "language" => prefs.language = Language::from_code(value),
                "theme" => {
                    prefs.theme = match value {
                        "dark" => ThemeChoice::Dark,
                        "light" => ThemeChoice::Light,
                        _ => ThemeChoice::System,
                    }
                }
                "scale" => {
                    if let Ok(scale) = value.parse::<f32>() {
                        if scale.is_finite() {
                            prefs.scale = scale.clamp(0.9, 1.3);
                        }
                    }
                }
                "large" => {
                    if let Ok(large) = value.parse::<u64>() {
                        prefs.large = large.clamp(1, 1_000_000);
                    }
                }
                "days" => {
                    if let Ok(days) = value.parse::<u64>() {
                        prefs.days = days.clamp(1, 36_500);
                    }
                }
                _ => {}
            }
        }
        prefs
    }
    fn encode(&self) -> String {
        format!(
            "language={}\ntheme={}\nscale={}\nlarge={}\ndays={}\n",
            self.language.code(),
            self.theme.code(),
            self.scale,
            self.large,
            self.days
        )
    }
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(folder) = Self::folder() {
            std::fs::create_dir_all(&folder)?;
            let temporary = folder.join("preferences.tmp");
            std::fs::write(&temporary, self.encode())?;
            std::fs::rename(temporary, folder.join("preferences"))?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_round_trip() {
        for language in Language::ALL {
            for theme in [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark] {
                let prefs = Preferences {
                    language,
                    theme,
                    scale: 1.2,
                    large: 2048,
                    days: 120,
                };
                assert_eq!(Preferences::parse(&prefs.encode()), prefs);
            }
        }
    }
    #[test]
    fn invalid_preferences_keep_usable_defaults() {
        let prefs = Preferences::parse(
            "language=bad\ntheme=bad\nscale=NaN\nlarge=0\ndays=999999\nunknown=x\nbroken",
        );
        assert_eq!(prefs.language, Language::Portuguese);
        assert_eq!(prefs.theme, ThemeChoice::System);
        assert_eq!(prefs.scale, 1.);
        assert_eq!(prefs.large, 1);
        assert_eq!(prefs.days, 36_500);
    }
}
