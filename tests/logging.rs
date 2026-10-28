use std::sync::{Arc, Mutex};

use matrix_bot_sdk::logging::{ConsoleLogger, ILogger, LogLevel, LogService, RichConsoleLogger};
use tracing::Level;

struct MockLogger {
    messages: Mutex<Vec<(Level, String, String)>>,
}

impl MockLogger {
    fn new() -> Self {
        Self {
            messages: Mutex::new(Vec::new()),
        }
    }

    fn messages(&self) -> Vec<(Level, String, String)> {
        self.messages.lock().unwrap().clone()
    }

    fn into_trait_object(self) -> Arc<dyn ILogger> {
        Arc::new(self)
    }
}

impl ILogger for MockLogger {
    fn log(&self, level: Level, module: &str, message: &str) {
        self.messages
            .lock()
            .unwrap()
            .push((level, module.to_owned(), message.to_owned()));
    }
}

#[test]
fn console_logger_can_be_created() {
    let _logger = ConsoleLogger::new();
}

#[test]
fn rich_console_logger_can_be_created() {
    let _logger = RichConsoleLogger::new();
}

#[test]
fn mock_logger_captures_messages() {
    let logger = MockLogger::new();

    logger.info("module", "test info message");
    logger.warn("module", "test warn message");
    logger.error("module", "test error message");
    logger.debug("module", "test debug message");
    logger.trace("module", "test trace message");

    let messages = logger.messages();
    assert_eq!(messages.len(), 5);
}

#[test]
fn log_service_can_be_created() {
    let logger = MockLogger::new().into_trait_object();
    let _service = LogService::new(logger);
}

struct SharedMockLogger {
    inner: Arc<MockLogger>,
}

impl SharedMockLogger {
    fn new() -> (Self, Arc<MockLogger>) {
        let inner = Arc::new(MockLogger::new());
        (
            Self {
                inner: Arc::clone(&inner),
            },
            inner,
        )
    }

    fn into_trait_object(self) -> Arc<dyn ILogger> {
        Arc::new(self)
    }
}

impl ILogger for SharedMockLogger {
    fn log(&self, level: Level, module: &str, message: &str) {
        self.inner.log(level, module, message);
    }
}

#[test]
fn log_service_forwards_info() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.info("module", "info message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, Level::INFO);
    assert_eq!(messages[0].2, "info message");
}

#[test]
fn log_service_forwards_warn() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.warn("module", "warn message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, Level::WARN);
    assert_eq!(messages[0].2, "warn message");
}

#[test]
fn log_service_forwards_error() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.error("module", "error message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0, Level::ERROR);
    assert_eq!(messages[0].2, "error message");
}

#[test]
fn log_service_returns_logger() {
    let (shared, mock) = SharedMockLogger::new();
    let logger: Arc<dyn ILogger> = shared.into_trait_object();
    let service = LogService::new(Arc::clone(&logger));

    let returned = service.logger();
    returned.info("module", "via returned logger");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
}

#[test]
fn log_service_multiple_messages() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.info("module", "message 1");
    service.info("module", "message 2");
    service.warn("module", "warning");
    service.error("module", "error");

    let messages = mock.messages();
    assert_eq!(messages.len(), 4);
}

#[test]
fn log_service_default_level_is_info() {
    let (shared, _) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());
    assert_eq!(service.level(), LogLevel::Info);
}

#[test]
fn log_service_can_set_level() {
    let (shared, _) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Debug);
    assert_eq!(service.level(), LogLevel::Debug);

    service.set_level(LogLevel::Error);
    assert_eq!(service.level(), LogLevel::Error);
}

#[test]
fn log_service_filters_by_level_trace_blocked_at_info() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Info);
    service.trace("module", "trace message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 0);
}

#[test]
fn log_service_filters_by_level_debug_blocked_at_info() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Info);
    service.debug("module", "debug message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 0);
}

#[test]
fn log_service_filters_by_level_info_blocked_at_warn() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Warn);
    service.info("module", "info message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 0);
}

#[test]
fn log_service_filters_by_level_warn_blocked_at_error() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Error);
    service.warn("module", "warn message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 0);
}

#[test]
fn log_service_allows_info_at_info_level() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Info);
    service.info("module", "info message");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
}

#[test]
fn log_service_mutes_module() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.mute_module("MutedModule");

    service.info("MutedModule", "should be muted");
    service.info("ActiveModule", "should appear");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].1, "ActiveModule");
}

#[test]
fn log_service_mutes_multiple_modules() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.mute_module("Muted1");
    service.mute_module("Muted2");

    service.info("Muted1", "muted");
    service.info("Muted2", "muted");
    service.info("Active", "active");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].1, "Active");
}

#[test]
fn log_service_unmutes_module() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.mute_module("TestModule");
    service.info("TestModule", "muted");

    service.unmute_module("TestModule");
    service.info("TestModule", "unmuted");

    let messages = mock.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].2, "unmuted");
}

#[test]
fn log_service_is_muted() {
    let (shared, _) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    assert!(!service.is_muted("TestModule"));

    service.mute_module("TestModule");
    assert!(service.is_muted("TestModule"));

    service.unmute_module("TestModule");
    assert!(!service.is_muted("TestModule"));
}

#[test]
fn log_service_silent_level_blocks_all() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Silent);

    service.trace("module", "trace");
    service.debug("module", "debug");
    service.info("module", "info");
    service.warn("module", "warn");
    service.error("module", "error");

    let messages = mock.messages();
    assert_eq!(messages.len(), 0);
}

#[test]
fn log_service_trace_level_allows_all() {
    let (shared, mock) = SharedMockLogger::new();
    let service = LogService::new(shared.into_trait_object());

    service.set_level(LogLevel::Trace);

    service.trace("module", "trace");
    service.debug("module", "debug");
    service.info("module", "info");
    service.warn("module", "warn");
    service.error("module", "error");

    let messages = mock.messages();
    assert_eq!(messages.len(), 5);
}
