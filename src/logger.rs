use std::io::Write;

use log::Level;
use chrono::Local;

pub fn configure_logger(base_level: Level) -> () {
    let actix_level = match base_level {
        Level::Info => Level::Warn,
        other_level => other_level,
    };
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(buf,
                "{} {} [{}] {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.target(),
                record.level(),
                record.args(),
            )
        })
        .filter_level(base_level.to_level_filter())
        .filter_module("actix_web::middleware::logger", actix_level.to_level_filter())
        .init();
}
