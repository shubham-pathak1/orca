use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RomanizationMode {
    #[default]
    Off,
    All,
    Selected,
}

impl<'de> Deserialize<'de> for RomanizationMode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Existing profiles used a boolean under `romanize_lyrics`.
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(match value {
            serde_json::Value::Bool(true) => Self::All,
            serde_json::Value::String(value) if value == "all" => Self::All,
            serde_json::Value::String(value) if value == "selected" => Self::Selected,
            _ => Self::Off,
        })
    }
}

impl RomanizationMode {
    pub fn index(self) -> i32 {
        match self {
            Self::Off => 0,
            Self::All => 1,
            Self::Selected => 2,
        }
    }
    pub fn from_index(index: i32) -> Self {
        match index {
            1 => Self::All,
            2 => Self::Selected,
            _ => Self::Off,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RomanizationPolicy {
    pub mode: RomanizationMode,
    pub languages: Vec<String>,
}

impl RomanizationPolicy {
    pub fn enabled(&self) -> bool {
        self.mode == RomanizationMode::All
            || self.mode == RomanizationMode::Selected
                && self
                    .languages
                    .iter()
                    .any(|code| romanization_language_code(code) != "und")
    }
}

pub use orca_core::romanization::canonical_language as romanization_language_code;

/// Source-language choices, not a guarantee of provider pronunciation coverage.
/// All mode also accepts provider languages beyond this picker.
pub const ROMANIZATION_LANGUAGES: &[(&str, &str)] = &[
    ("am", "Amharic"),
    ("ar", "Arabic"),
    ("hy", "Armenian"),
    ("as", "Assamese"),
    ("az", "Azerbaijani"),
    ("be", "Belarusian"),
    ("bn", "Bengali"),
    ("bho", "Bhojpuri"),
    ("bg", "Bulgarian"),
    ("my", "Burmese"),
    ("zh", "Chinese"),
    ("dv", "Dhivehi"),
    ("doi", "Dogri"),
    ("dz", "Dzongkha"),
    ("ka", "Georgian"),
    ("el", "Greek"),
    ("gu", "Gujarati"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("ja", "Japanese"),
    ("kn", "Kannada"),
    ("kk", "Kazakh"),
    ("km", "Khmer"),
    ("gom", "Konkani"),
    ("ko", "Korean"),
    ("ckb", "Kurdish (Sorani)"),
    ("ky", "Kyrgyz"),
    ("lo", "Lao"),
    ("mk", "Macedonian"),
    ("mai", "Maithili"),
    ("ml", "Malayalam"),
    ("mni", "Manipuri"),
    ("mr", "Marathi"),
    ("mn", "Mongolian"),
    ("ne", "Nepali"),
    ("or", "Odia"),
    ("ps", "Pashto"),
    ("fa", "Persian"),
    ("pa", "Punjabi"),
    ("ru", "Russian"),
    ("sa", "Sanskrit"),
    ("sat", "Santali"),
    ("sr", "Serbian"),
    ("sd", "Sindhi"),
    ("si", "Sinhala"),
    ("tg", "Tajik"),
    ("ta", "Tamil"),
    ("tt", "Tatar"),
    ("te", "Telugu"),
    ("th", "Thai"),
    ("bo", "Tibetan"),
    ("ti", "Tigrinya"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("ug", "Uyghur"),
    ("uz", "Uzbek"),
    ("yi", "Yiddish"),
];
