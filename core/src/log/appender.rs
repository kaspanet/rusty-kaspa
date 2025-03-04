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
    encode::{pattern::PatternEncoder, Color, Encode, Style, Write},
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
            Box::new(ConsoleAppender::builder().encoder(Box::new(CrescendoEncoder::new(LOG_LINE_PATTERN_COLORED))).build()),
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

pub const CRESCENDO_KEYWORD: &str = "crescendo";
const CRESCENDO_LOG_LINE_PATTERN_COLORED: &str = "{d(%Y-%m-%d %H:%M:%S%.3f%:z)} [{h({(CRND):5.5})}] {m}{n}";

// TODO (post HF): remove or hide the custom encoder
#[derive(Debug)]
struct CrescendoEncoder {
    general_encoder: PatternEncoder,
    crescendo_encoder: PatternEncoder,
    keyword: &'static str,
}

impl CrescendoEncoder {
    fn new(pattern: &str) -> Self {
        CrescendoEncoder {
            general_encoder: PatternEncoder::new(pattern),
            crescendo_encoder: PatternEncoder::new(CRESCENDO_LOG_LINE_PATTERN_COLORED),
            keyword: CRESCENDO_KEYWORD,
        }
    }
}

impl Encode for CrescendoEncoder {
    fn encode(&self, w: &mut dyn Write, record: &log::Record) -> anyhow::Result<()> {
        if record.target() == self.keyword {
            // Hack: override log level to debug so that inner encoder does not reset the style
            // (note that we use the custom pattern with CRND so this change isn't visible)
            let record = record.to_builder().level(log::Level::Debug).build();
            w.set_style(Style::new().text(Color::Cyan))?;
            self.crescendo_encoder.encode(w, &record)?;
            w.set_style(&Style::new())?;
            Ok(())
        } else {
            self.general_encoder.encode(w, record)
        }
    }
}
