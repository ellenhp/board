use std::time::Duration;

/// Periodically sync the system clock from an HTTP Date header.
/// Runs forever, syncing every 5 minutes. Errors are silently ignored
/// since the OBA clock offset provides a fallback.
pub async fn run_clock_sync() {
    loop {
        let _ = sync_once().await;
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn sync_once() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client.head("http://one.one.one.one").send().await?;

    let date_str = resp
        .headers()
        .get("date")
        .ok_or("no date header")?
        .to_str()?;

    // HTTP date format: "Mon, 16 Feb 2026 01:11:59 GMT"
    // Replace GMT with +0000 so chrono's RFC 2822 parser accepts it
    let dt = chrono::DateTime::parse_from_rfc2822(&date_str.replace("GMT", "+0000"))?;

    let ts = libc::timespec {
        tv_sec: dt.timestamp() as _,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) };
    if ret != 0 {
        return Err("clock_settime failed".into());
    }

    Ok(())
}
