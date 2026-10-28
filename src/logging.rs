use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use tracing::Level;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    #[default]
    Info = 2,
    Warn = 3,
    Error = 4,
    Silent = 5,
}

pub trait ILogger: Send + Sync {
    fn log(&self, level: Level, module: &str, message: &str);

    fn trace(&self, module: &str, message: &str) {
        self.log(Level::TRACE, module, message);
    }

    fn debug(&self, module: &str, message: &str) {
        self.log(Level::DEBUG, module, message);
    }

    fn info(&self, module: &str, message: &str) {
        self.log(Level::INFO, module, message);
    }

    fn warn(&self, module: &str, message: &str) {
        self.log(Level::WARN, module, message);
    }

    fn error(&self, module: &str, message: &str) {
        self.log(Level::ERROR, module, message);
    }
}

#[derive(Default)]
pub struct ConsoleLogger;

impl ConsoleLogger {
    pub fn new() -> Self {
        Self
    }
}

impl ILogger for ConsoleLogger {
    fn log(&self, level: Level, module: &str, message: &str) {
        match level {
            Level::TRACE => tracing::trace!("[{module}] {message}"),
            Level::DEBUG => tracing::debug!("[{module}] {message}"),
            Level::INFO => tracing::info!("[{module}] {message}"),
            Level::WARN => tracing::warn!("[{module}] {message}"),
            Level::ERROR => tracing::error!("[{module}] {message}"),
        }
    }
}

#[derive(Default)]
pub struct RichConsoleLogger;

impl RichConsoleLogger {
    pub fn new() -> Self {
        Self
    }
}

impl ILogger for RichConsoleLogger {
    fn log(&self, level: Level, module: &str, message: &str) {
        let (prefix, color) = match level {
            Level::TRACE => ("TRACE", "\x1b[90m"),
            Level::DEBUG => ("DEBUG", "\x1b[36m"),
            Level::INFO => ("INFO ", "\x1b[32m"),
            Level::WARN => ("WARN ", "\x1b[33m"),
            Level::ERROR => ("ERROR", "\x1b[31m"),
        };
        println!("{color}[{prefix}] [{module}] {message}\x1b[0m");
    }
}

#[derive(Clone)]
pub struct LogService {
    logger: Arc<dyn ILogger>,
    level: Arc<RwLock<LogLevel>>,
    muted_modules: Arc<RwLock<HashSet<String>>>,
}

impl LogService {
    pub fn new(logger: Arc<dyn ILogger>) -> Self {
        Self {
            logger,
            level: Arc::new(RwLock::new(LogLevel::default())),
            muted_modules: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn logger(&self) -> Arc<dyn ILogger> {
        Arc::clone(&self.logger)
    }

    pub fn set_level(&self, level: LogLevel) {
        *self.level.write().unwrap() = level;
    }

    pub fn level(&self) -> LogLevel {
        *self.level.read().unwrap()
    }

    pub fn mute_module(&self, module: &str) {
        self.muted_modules
            .write()
            .unwrap()
            .insert(module.to_owned());
    }

    pub fn unmute_module(&self, module: &str) {
        self.muted_modules.write().unwrap().remove(module);
    }

    pub fn is_muted(&self, module: &str) -> bool {
        self.muted_modules.read().unwrap().contains(module)
    }

    fn should_log(&self, level: LogLevel, module: &str) -> bool {
        if self.is_muted(module) {
            return false;
        }
        let current_level = self.level();
        level >= current_level
    }

    pub fn trace(&self, module: &str, message: &str) {
        if self.should_log(LogLevel::Trace, module) {
            self.logger.trace(module, message);
        }
    }

    pub fn debug(&self, module: &str, message: &str) {
        if self.should_log(LogLevel::Debug, module) {
            self.logger.debug(module, message);
        }
    }

    pub fn info(&self, module: &str, message: &str) {
        if self.should_log(LogLevel::Info, module) {
            self.logger.info(module, message);
        }
    }

    pub fn warn(&self, module: &str, message: &str) {
        if self.should_log(LogLevel::Warn, module) {
            self.logger.warn(module, message);
        }
    }

    pub fn error(&self, module: &str, message: &str) {
        if self.should_log(LogLevel::Error, module) {
            self.logger.error(module, message);
        }
    }
}
