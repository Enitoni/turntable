use std::fmt::Display;

use colored::Colorize;
use log::Level;

/// External crates only need to log warnings and errors
const ALLOWED_EXTERNAL_LEVELS: [Level; 2] = [Level::Warn, Level::Error];
const ALLOWED_LEVELS: [Level; 3] = [Level::Info, Level::Warn, Level::Error];

pub fn init_logger() {
    fern::Dispatch::new()
        .format(move |out, message, record| {
            let target = Target::from_str(record.target());
            let now = chrono::Local::now();

            out.finish(format_args!(
                "{:^5} {} {:^8} {}",
                level_to_string(&record.level()),
                now.format("%H:%M:%S").to_string().bright_black(),
                target,
                message
            ))
        })
        .filter(|meta| {
            let target = Target::from_str(meta.target());

            let is_allowed = ALLOWED_LEVELS.contains(&meta.level());
            let is_severe = ALLOWED_EXTERNAL_LEVELS.contains(&meta.level());

            target.is_local() && is_allowed || is_severe
        })
        .chain(std::io::stdout())
        .apply()
        .expect("logging is initialized")
}

enum Target {
    External(String),
    Server,
    Collab,
    Core,
}

impl Target {
    fn from_str(str: &str) -> Self {
        let mut split = str.split("::");
        let module = split.next().unwrap();

        match module {
            "turntable_core" => Self::Core,
            "turntable_server" => Self::Server,
            "turntable_collab" => Self::Collab,
            other => Target::External(other.to_string()),
        }
    }

    fn is_local(&self) -> bool {
        !matches!(self, Self::External(_))
    }
}

impl Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            Target::External(x) => x.as_str().clear(),
            Target::Server => "SERVER".bright_green(),
            Target::Collab => "COLLAB".bright_purple(),
            Target::Core => "CORE".blue(),
        };

        Display::fmt(&result, f)
    }
}

fn level_to_string(level: &Level) -> String {
    match level {
        Level::Error => " ERR ".black().on_red().bold().to_string(),
        Level::Warn => " WRN ".black().on_yellow().bold().to_string(),
        Level::Info => " INF ".black().on_blue().bold().to_string(),
        Level::Debug => " DBG ".white().on_black().to_string(),
        Level::Trace => " TRC ".to_string(),
    }
}
