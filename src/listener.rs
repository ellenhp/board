#[cfg(feature = "reminder")]
pub async fn listen_udp() -> anyhow::Result<()> {
    use tokio::net::UdpSocket;
    let sock = UdpSocket::bind("0.0.0.0:1337").await?;
    let mut buf = [0; 1024];
    loop {
        let (len, _addr) = sock.recv_from(&mut buf).await?;
        let stop = String::from_utf8_lossy(&buf[0..len]).trim().to_string();

        if !stop.chars().all(|c| c.is_alphanumeric()) || stop.len() > 10 {
            continue;
        }

        use std::time::Duration;

        tokio::time::sleep(Duration::from_secs(3)).await;

        use subprocess::Exec;
        tokio::spawn(async {
            let _ = Exec::cmd("aplay").arg("remind.wav").start();
        });

        tokio::time::sleep(Duration::from_secs(3)).await;

        let req_path = format!("{}.req", stop);

        std::fs::write(req_path, &[])?;
    }
}
