use chrono::Local;
use std::io::Write;

pub fn configure_logger() -> () {
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
        .filter(None, log::LevelFilter::Info)
        .init();
}
