use log::LevelFilter;
use log4rs::Config;
use log4rs::append::Append;
use log4rs::config::{Appender, Root};
use log4rs::encode::Encode;
use log4rs::encode::pattern::PatternEncoder;
use log4rs::encode::writer::simple::SimpleWriter;
use tokio::sync::mpsc::Sender;

pub use log;
pub use log4rs;

pub fn init_stdout_logger(level: LevelFilter) {
    let stdout = log4rs::append::console::ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{m}{n}")))
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(level))
        .unwrap();

    log4rs::init_config(config).ok();
}

pub fn init_file_logger(path: &str, level: LevelFilter) {
    use log4rs::append::rolling_file::RollingFileAppender;
    use log4rs::append::rolling_file::policy::compound::CompoundPolicy;
    use log4rs::append::rolling_file::policy::compound::roll::fixed_window::FixedWindowRoller;
    use log4rs::append::rolling_file::policy::compound::trigger::size::SizeTrigger;

    let roller = FixedWindowRoller::builder()
        .build(&format!("{}.{{}}.gz", path), 2)
        .unwrap();

    let trigger = SizeTrigger::new(128 * 1024 * 1024);

    let policy = CompoundPolicy::new(Box::new(trigger), Box::new(roller));

    let file = RollingFileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%dT%H:%M:%S)} [{l}] {m}{n}",
        )))
        .build(path, Box::new(policy))
        .unwrap();

    let config = Config::builder()
        .appender(Appender::builder().build("file", Box::new(file)))
        .build(Root::builder().appender("file").build(level))
        .unwrap();

    log4rs::init_config(config).ok();
}

pub fn init_stderr_logger(level: LevelFilter) {
    let stderr = log4rs::append::console::ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{m}{n}")))
        .target(log4rs::append::console::Target::Stderr)
        .build();

    let config = Config::builder()
        .appender(Appender::builder().build("stderr", Box::new(stderr)))
        .build(Root::builder().appender("stderr").build(level))
        .unwrap();

    log4rs::init_config(config).ok();
}

pub fn init_channel_logger(tx: Sender<String>, level: LevelFilter) {
    let appender = ChannelAppender::new(tx);

    let config = Config::builder()
        .appender(Appender::builder().build("channel", Box::new(appender)))
        .build(Root::builder().appender("channel").build(level))
        .unwrap();

    log4rs::init_config(config).ok();
}

#[derive(Debug)]
pub struct ChannelAppender {
    tx: Sender<String>,
    encoder: PatternEncoder,
}

impl ChannelAppender {
    pub fn new(tx: Sender<String>) -> Self {
        Self {
            tx,
            encoder: PatternEncoder::new("{m}"),
        }
    }
}

impl Append for ChannelAppender {
    fn append(&self, record: &log::Record) -> anyhow::Result<()> {
        let mut buf = Vec::new();
        self.encoder.encode(&mut SimpleWriter(&mut buf), record)?;
        let message = String::from_utf8_lossy(&buf).to_string();
        let _ = self.tx.try_send(message);
        Ok(())
    }

    fn flush(&self) {}
}
