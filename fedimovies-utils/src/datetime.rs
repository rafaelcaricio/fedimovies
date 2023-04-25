use chrono::{DateTime, Duration, NaiveDateTime, Utc};

pub fn get_min_datetime() -> DateTime<Utc> {
    let native = NaiveDateTime::from_timestamp_opt(0, 0).expect("0 should be a valid argument");
    DateTime::from_utc(native, Utc)
}

pub fn days_before_now(days: u32) -> DateTime<Utc> {
    Utc::now() - Duration::days(days.into())
}
