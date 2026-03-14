use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::Arrival;

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(rename = "currentTime")]
    current_time: i64,
    data: ApiData,
}

#[derive(Debug, Deserialize)]
struct ApiData {
    entry: ApiEntry,
    references: ApiReferences,
}

#[derive(Debug, Deserialize)]
struct ApiReferences {
    routes: Vec<ApiRoute>,
}

#[derive(Debug, Deserialize)]
struct ApiRoute {
    id: String,
    #[serde(rename = "shortName", default)]
    short_name: String,
    #[serde(default)]
    color: String,
}

#[derive(Debug, Deserialize)]
struct ApiEntry {
    #[serde(rename = "arrivalsAndDepartures")]
    arrivals_and_departures: Vec<ArrivalDeparture>,
}

#[derive(Debug, Deserialize)]
struct ArrivalDeparture {
    #[serde(rename = "predictedArrivalTime")]
    predicted_arrival_time: i64,
    #[serde(rename = "scheduledArrivalTime")]
    scheduled_arrival_time: i64,
    #[serde(rename = "tripHeadsign")]
    trip_headsign: String,
    #[serde(rename = "routeId")]
    route_id: String,
    #[serde(rename = "tripId")]
    trip_id: String,
    predicted: Option<bool>,
}

fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

fn api_token() -> String {
    std::env::var("OBA_API_KEY").expect("OBA_API_KEY environment variable must be set")
}

/// Fetch arrivals for a single stop. Returns the arrivals and a clock offset
/// (server_time - local_time) in ms for correcting drift on devices without NTP.
pub async fn fetch_arrivals(stop_id: &str) -> Result<(Vec<Arrival>, i64), reqwest::Error> {
    let url = format!(
        "https://api.pugetsound.onebusaway.org/api/where/arrivals-and-departures-for-stop/{}.json?key={}",
        stop_id,
        api_token()
    );

    let response: ApiResponse = reqwest::get(&url).await?.json().await?;

    let local_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let clock_offset = response.current_time - local_ms;

    let server_now = response.current_time;
    let routes = &response.data.references.routes;

    let arrivals = response
        .data
        .entry
        .arrivals_and_departures
        .into_iter()
        .filter(|arrival| arrival.predicted == Some(true))
        .map(|arr| {
            let arrival_time = if arr.predicted_arrival_time > 0 {
                arr.predicted_arrival_time
            } else {
                arr.scheduled_arrival_time
            };
            let minutes = (arrival_time - server_now) / 60000;
            let destination = arr
                .trip_headsign
                .trim_end_matches(" City Center")
                .trim_end_matches(" Downtown")
                .replace("Int'l Dist/Chinatown", "Int'l District")
                .to_string();

            let route = routes.iter().find(|r| r.id == arr.route_id);
            let route_label = route
                .map(|r| r.short_name.split_whitespace().next().unwrap_or("?"))
                .unwrap_or("?")
                .to_string();
            let route_color = route
                .and_then(|r| parse_hex_color(&r.color))
                .unwrap_or([0x80, 0x80, 0x80]);

            Arrival {
                destination,
                arrival_time_ms: arrival_time,
                minutes,
                route_id: arr.route_id,
                route_label,
                route_color,
                trip_id: arr.trip_id,
                stop_id: stop_id.to_string(),
            }
        })
        .collect();

    Ok((arrivals, clock_offset))
}

/// `fetch_arrivals` with up to 3 attempts (500 ms delay between retries).
pub async fn fetch_with_retry(stop_id: &str) -> Result<(Vec<Arrival>, i64), reqwest::Error> {
    for attempt in 0..3 {
        match fetch_arrivals(stop_id).await {
            Ok(result) => return Ok(result),
            Err(_) if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
