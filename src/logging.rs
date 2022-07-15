use std::fmt::{Debug, Display};

use colored::{Color, Colorize};
use log::{Level, LevelFilter};

pub fn init_logger() {
    fern::Dispatch::new()
        .format(move |out, message, record| {
            let target = Target::from_str(record.target());
            let now = chrono::Local::now();

            let level = match record.level() {
                log::Level::Error => "ERR".color(LogColor::Black).on_color(LogColor::Red).bold(),
                log::Level::Warn => "WRN"
                    .color(LogColor::Black)
                    .on_color(LogColor::Orange)
                    .bold(),
                log::Level::Info => "INF".color(LogColor::Black).on_color(LogColor::Blue).bold(),
                log::Level::Debug => {
                    let index = (now.timestamp_subsec_micros() as usize) % DEBUG_WORDS.len();
                    let word = DEBUG_WORDS[index];

                    word.color(LogColor::Black).on_color(LogColor::Teal).bold()
                }
                log::Level::Trace => "TRC".bold(),
            };

            let message_color = if target.is_important() && record.level() != Level::Trace {
                LogColor::White.into()
            } else {
                LogColor::White.dimmed()
            };

            out.finish(format_args!(
                "{:^5} {} {:.<7} {}",
                level,
                now.format("%H:%M:%S")
                    .to_string()
                    .color(LogColor::White.dimmed()),
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
const DEBUG_WORDS: [&str; 7] = ["FCK", "SHT", "ASS", "WHY", "WTF", "NOO", "AGH"];

// External libraries don't need to log unless it is important
const ALLOWED_LEVELS: [Level; 2] = [Level::Warn, Level::Error];

#[derive(Clone)]
enum Target {
    External(String),
    Discord,
    Server,
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
                "http" => Self::Server,
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
        let color: LogColor = self.clone().into();

        let result = match self {
            Target::External(x) => x.as_str().clear(),
            Target::Crate => "LOCAL".clear(),
            Target::Other => "OTHER".clear(),

            Target::Discord => "DISCORD".color(color),
            Target::Server => "SERVER".color(color),
            Target::Audio => "AUDIO".color(color),
        };

        Display::fmt(&result, f)
    }
}

impl From<Target> for LogColor {
    fn from(target: Target) -> Self {
        match target {
            Target::Discord => LogColor::Blurple,
            Target::Server => LogColor::LightGreen,
            Target::Audio => LogColor::Magenta,
            _ => LogColor::White,
        }
    }
}

pub enum LogColor {
    Red,
    Teal,
    Blue,
    Black,
    White,
    Dimmed,
    Orange,
    Magenta,
    Blurple,
    LightGreen,
    Success,
}

impl LogColor {
    const fn values(&self) -> (u8, u8, u8) {
        match self {
            LogColor::Red => (255, 73, 13),
            LogColor::Teal => (252, 177, 3),
            LogColor::Blue => (0, 200, 255),
            LogColor::White => (255, 255, 255),
            LogColor::Black => (0, 0, 0),
            LogColor::Dimmed => (70, 70, 70),
            LogColor::Orange => (252, 177, 3),
            LogColor::Magenta => (207, 105, 255),
            LogColor::Blurple => (88, 101, 242),
            LogColor::Success => (112, 250, 150),
            LogColor::LightGreen => (112, 250, 150),
        }
    }

    const fn dimmed(&self) -> Color {
        let (r, g, b) = self.values();
        let dim_value = 2;

        let r = r / dim_value;
        let g = g / dim_value;
        let b = b / dim_value;

        Color::TrueColor { r, g, b }
    }
}

impl From<LogColor> for Color {
    fn from(color: LogColor) -> Self {
        let (r, g, b) = color.values();
        Color::TrueColor { r, g, b }
    }
}
