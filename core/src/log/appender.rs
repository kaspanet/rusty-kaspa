use super::consts::{
    LOG_ARCHIVE_SUFFIX, LOG_FILE_BASE_ROLLS, LOG_FILE_MAX_ROLLS, LOG_FILE_MAX_SIZE, LOG_LINE_PATTERN, LOG_LINE_PATTERN_COLORED,
};
use log::LevelFilter;
use log4rs::{
    append::{
        console::ConsoleAppender,
        rolling_file::{
            policy::compound::{roll::fixed_window::FixedWindowRoller, trigger::size::SizeTrigger, CompoundPolicy},
            RollingFileAppender,
        },
        Append,
    },
    config::Appender,
    encode::pattern::PatternEncoder,
    filter::{threshold::ThresholdFilter, Filter},
};
use std::path::PathBuf;

pub(super) struct AppenderSpec {
    pub name: &'static str,
    level: Option<LevelFilter>,
    append: Option<Box<dyn Append>>,
}

impl AppenderSpec {
    pub fn console(name: &'static str, level: Option<LevelFilter>) -> Self {
        Self::new(
            name,
            level,
            Box::new(ConsoleAppender::builder().encoder(Box::new(PatternEncoder::new(LOG_LINE_PATTERN_COLORED))).build()),
        )
    }

    pub fn roller(name: &'static str, level: Option<LevelFilter>, log_dir: &str, file_name: &str) -> Self {
        let appender = {
            let trigger = Box::new(SizeTrigger::new(LOG_FILE_MAX_SIZE));

            let file_path = PathBuf::from(log_dir).join(file_name);
            let roller_pattern = PathBuf::from(log_dir).join(format!("{}{}", file_name, LOG_ARCHIVE_SUFFIX));
            let roller = Box::new(
                FixedWindowRoller::builder()
                    .base(LOG_FILE_BASE_ROLLS)
                    .build(roller_pattern.to_str().unwrap(), LOG_FILE_MAX_ROLLS)
                    .unwrap(),
            );

            let compound_policy = Box::new(CompoundPolicy::new(trigger, roller));
            let file_appender = RollingFileAppender::builder()
                .encoder(Box::new(PatternEncoder::new(LOG_LINE_PATTERN)))
                .build(file_path, compound_policy)
                .unwrap();

            Box::new(file_appender) as Box<dyn Append>
        };
        Self::new(name, level, appender)
    }

    pub fn new(name: &'static str, level: Option<LevelFilter>, append: Box<dyn Append>) -> Self {
        Self { name, level, append: Some(append) }
    }

    pub fn appender(&mut self) -> Appender {
        Appender::builder()
            .filters(self.level.map(|x| Box::new(ThresholdFilter::new(x)) as Box<dyn Filter>))
            .build(self.name, self.append.take().unwrap())
    }
}
