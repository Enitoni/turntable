use std::fmt::{Debug, Display};

use colored::{Color, Colorize};
use log::{Level, LevelFilter};

pub fn init_logger() {
    fern::Dispatch::new()
        .format(move |out, message, record| {
            let target = Target::from_str(record.target());
            let now = chrono::Local::now();

            let level = match record.level() {
                log::Level::Error => "ERR!".color(BLACK).on_color(RED).bold(),
                log::Level::Warn => "WARN".color(BLACK).on_color(ORANGE).bold(),
                log::Level::Info => "INFO".color(BLACK).on_color(BLUE).bold(),
                log::Level::Debug => {
                    let index = (now.timestamp_subsec_micros() as usize) % DEBUG_WORDS.len();
                    let word = DEBUG_WORDS[index];

                    word.color(BLACK).on_color(TEAL).bold()
                }
                log::Level::Trace => "LOG".color(DIMMED).bold(),
            };

            let message_color = if target.is_important() {
                Color::White
            } else {
                DIMMED
            };

            out.finish(format_args!(
                "{:^6} {} {:<6} {}",
                level,
                now.format("%H:%M:%S").to_string().color(DIMMED),
                target,
                message.to_string().color(message_color)
            ))
        })
        .filter(|meta| {
            let is_important = Target::from_str(meta.target()).is_important();
            let is_severe = ALLOWED_LEVELS.contains(&meta.level());

            is_important || is_severe
        })
        .chain(std::io::stdout())
        .apply()
        .unwrap()
}

// Programmers are very peaceful creatures.
const DEBUG_WORDS: [&str; 7] = ["FUCK", "SHIT", "GRR!", "WHY?", "AAAA", "NOPE", "COCK"];

// External libraries don't need to log unless it is important
const ALLOWED_LEVELS: [Level; 2] = [Level::Warn, Level::Error];

enum Target {
    External(String),
    Discord,
    Crate,
    Audio,
    Other,
}

impl Target {
    fn from_str(str: &str) -> Self {
        let mut split = str.split("::");

        let module = split.next().unwrap();
        let child = split.next();

        if module != env!("CARGO_PKG_NAME") {
            return Self::External(module.to_string());
        }

        if let Some(child) = child {
            return match child {
                "audio" => Self::Audio,
                "discord" => Self::Discord,
                _ => Self::Other,
            };
        }

        Self::Crate
    }

    fn is_local(&self) -> bool {
        !matches!(self, Self::External(_))
    }

    fn is_important(&self) -> bool {
        self.is_local() && !matches!(self, Self::Other)
    }
}

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            Target::External(x) => x.color(Color::White),
            Target::Discord => "DISCORD".color(BLURPLE),
            Target::Crate => "LOCAL".color(Color::White),
            Target::Audio => "AUDIO".color(MAGENTA),
            Target::Other => "OTHER".color(Color::White),
        };

        let result = format!("{}:", result);
        Display::fmt(&result, f)
    }
}

const RED: Color = Color::TrueColor {
    r: 255,
    g: 73,
    b: 13,
};

const ORANGE: Color = Color::TrueColor {
    r: 252,
    g: 177,
    b: 3,
};

const BLACK: Color = Color::TrueColor { r: 0, g: 0, b: 0 };

const DIMMED: Color = Color::TrueColor {
    r: 70,
    g: 70,
    b: 70,
};

const BLUE: Color = Color::TrueColor {
    r: 0,
    g: 200,
    b: 255,
};

const TEAL: Color = Color::TrueColor {
    r: 0,
    g: 255,
    b: 200,
};

const MAGENTA: Color = Color::TrueColor {
    r: 210,
    g: 64,
    b: 255,
};

const BLURPLE: Color = Color::TrueColor {
    r: 88,
    g: 101,
    b: 242,
};
