pub mod clock;
mod oba;

pub use oba::{fetch_arrivals, fetch_with_retry};

#[derive(Debug, Clone)]
pub struct Arrival {
    pub destination: String,
    pub arrival_time_ms: i64,
    pub minutes: i64,
    pub route_id: String,
    pub route_label: String,
    pub route_color: [u8; 3],
    pub trip_id: String,
    pub stop_id: String,
}

/// Recalculate `minutes` using local time + clock offset, then sort, filter,
/// and dedup. `clock_offset_ms` is server_time - local_time, learned from the
/// API's `currentTime` field to correct for drift on devices without NTP.
pub fn recalculate_and_filter(arrivals: &mut Vec<Arrival>, clock_offset_ms: i64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        + clock_offset_ms;
    for a in arrivals.iter_mut() {
        a.minutes = (a.arrival_time_ms - now) / 60000;
    }
    arrivals.sort_by_key(|a| a.arrival_time_ms);
    arrivals.retain(|a| a.minutes >= -1 && a.minutes <= 45);
    arrivals.dedup_by(|a, b| {
        a.minutes == b.minutes && a.route_id == b.route_id && a.destination == b.destination
    });
}

/// Format minutes until arrival for display: "Now" when <= 0, otherwise "Xm".
pub fn format_time(minutes: i64) -> String {
    if minutes <= 0 {
        "Now".to_string()
    } else {
        format!("{}m", minutes)
    }
}
