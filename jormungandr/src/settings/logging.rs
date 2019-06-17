use crate::log::{AsyncableDrain, JsonDrain};
use hostname::get_hostname;
use slog::{Drain, Logger};
use slog_async::Async;
use slog_gelf::Gelf;
#[cfg(feature = "systemd")]
use slog_journald::JournaldDrain;
#[cfg(unix)]
use slog_syslog::Facility;
use std::io;
use std::str::FromStr;

#[derive(Debug)]
pub struct LogSettings {
    pub verbosity: slog::Level,
    pub format: LogFormat,
    pub output: LogOutput,
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// Format of the logger.
pub enum LogFormat {
    Plain,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
/// Output of the logger.
pub enum LogOutput {
    Stderr,
    Gelf,
    #[cfg(unix)]
    Syslog,
    #[cfg(feature = "systemd")]
    Journald,
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &*s.trim().to_lowercase() {
            "plain" => Ok(LogFormat::Plain),
            "json" => Ok(LogFormat::Json),
            other => Err(format!("unknown log format '{}'", other)),
        }
    }
}

impl FromStr for LogOutput {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &*s.trim().to_lowercase() {
            "stderr" => Ok(LogOutput::Stderr),
            "gelf" => Ok(LogOutput::Gelf),
            #[cfg(unix)]
            "syslog" => Ok(LogOutput::Syslog),
            #[cfg(feature = "systemd")]
            "journald" => Ok(LogOutput::Journald),
            other => Err(format!("unknown log output '{}'", other)),
        }
    }
}

impl LogSettings {
    pub fn to_logger(&self) -> Result<Logger, Error> {
        let drain = self.output.to_logger(&self.format, &self.backend)?.fuse();
        let drain = slog::LevelFilter::new(drain, self.verbosity).fuse();
        Ok(slog::Logger::root(drain, o!()))
    }
}

impl LogOutput {
    fn to_logger(&self, format: &LogFormat, backend: &Option<String>) -> Result<Async, Error> {
        match self {
            LogOutput::Stderr => Ok(format.decorate_stderr()),
            LogOutput::Gelf => match backend {
                Some(graylog_host_port) => {
                    // We use hostname as source for now, but a better identifier should be used.
                    let source = get_hostname().unwrap_or("unhostnamed".to_string());
                    let gelf_drain = Gelf::new(&source, graylog_host_port).unwrap().fuse();
                    Ok(format.decorate(gelf_drain))
                }
                _ => Err(Error::MissingGelfBackend),
            },
            #[cfg(unix)]
            LogOutput::Syslog => Ok(format.decorate(slog_syslog::unix_3164(Facility::LOG_USER)?)),
            #[cfg(feature = "systemd")]
            LogOutput::Journald => Ok(format.decorate(JournaldDrain)),
        }
    }
}

impl LogFormat {
    fn decorate_stderr(&self) -> Async {
        match self {
            LogFormat::Plain => slog_term::term_full().async(),
            LogFormat::Json => slog_json::Json::default(io::stderr()).async(),
        }
    }

    fn decorate<D: AsyncableDrain>(&self, drain: D) -> Async
    where
        <D as Drain>::Err: std::fmt::Debug,
    {
        match self {
            LogFormat::Plain => drain.async(),
            LogFormat::Json => JsonDrain::new(drain).async(),
        }
    }
}

custom_error! {pub Error
    SyslogAccessFailed { source: io::Error } = "syslog access failed",
    MissingGelfBackend = "Please specify a backend (host:port of graylog server) for the GELF logger output",
}
