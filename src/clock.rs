use std::time::Duration;

/// Periodically sync the system clock from an HTTP Date header.
/// Runs forever, syncing every 5 minutes. Errors are silently ignored
/// since the OBA clock offset provides a fallback.
pub async fn run_clock_sync() {
    loop {
        if let Err(err) = sync_once().await {
            eprintln!("Error getting time from cloudflare: {:?}", err);
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

async fn sync_once() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        // This is fine because we're only using this client for HTTP requests
        // anyway so a MITM attack could just drop TLS anyway. As it stands,
        // the cert provided is rejected on the PM3 because of outdated roots.
        .danger_accept_invalid_certs(true)
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
