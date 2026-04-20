pub const EQ_DEFAULT_GAINS: [f32; 5] = [0.0, 0.0, 0.0, 0.0, 0.0];

pub fn eq_preset_from_index(index: i32) -> (&'static str, [f32; 5]) {
    match index {
        1 => ("Bass Boost", [5.0, 3.0, 0.0, -1.0, -2.0]),
        2 => ("Vocal Focus", [-2.0, 0.0, 3.0, 2.0, -1.0]),
        3 => ("Treble Boost", [-2.0, -1.0, 0.0, 3.0, 5.0]),
        4 => ("V Shape", [4.0, 2.0, -2.0, 2.0, 4.0]),
        _ => ("Flat", EQ_DEFAULT_GAINS),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RepeatMode {
    Off,
    One,
    All,
}

impl RepeatMode {
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::One,
            Self::One => Self::All,
            Self::All => Self::Off,
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::One => "one",
            Self::All => "all",
        }
    }

    pub fn from_setting(value: &str) -> Self {
        match value {
            "one" => Self::One,
            "all" => Self::All,
            _ => Self::Off,
        }
    }
}
