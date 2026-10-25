use anyhow::{Context, Result, bail, ensure};
use lanprint::{AppState, web, worker};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::atomic::Ordering,
};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lanprint=info".into()),
        )
        .init();
    let mut data = PathBuf::from("./data");
    let mut bind: SocketAddr = "127.0.0.1:8787".parse()?;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => data = args.next().context("Missing --data-dir value")?.into(),
            "--bind" => bind = args.next().context("Missing --bind value")?.parse()?,
            "--help" | "-h" => {
                println!(
                    "lanprint [--data-dir ./data] [--bind 127.0.0.1:8787]\nBind to loopback (default) or an explicit RFC1918 private IPv4 address. LAN access has no authentication and uses plain HTTP; use only on a trusted network."
                );
                return Ok(());
            }
            _ => bail!("Unknown argument: {arg}"),
        }
    }
    validate_bind_address(bind)?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let address = listener.local_addr()?;
    let state = AppState::open(
        &data,
        vec![address.to_string(), format!("localhost:{}", address.port())],
    )?;
    let worker = tokio::spawn(worker(state.clone()));
    tracing::info!(url=%format!("http://{address}"),data=%state.data_dir.display(),"lanprint ready; monitoring paused");
    let result = axum::serve(listener, web::router(state.clone()))
        .with_graceful_shutdown(shutdown_signal())
        .await;
    state.shutdown.store(true, Ordering::SeqCst);
    state.control.lock().unwrap().monitoring = false;
    tracing::info!("waiting for active print operation to finish");
    worker.await?;
    result?;
    Ok(())
}

fn validate_bind_address(bind: SocketAddr) -> Result<()> {
    ensure!(
        bind.ip().is_loopback() || matches!(bind.ip(), IpAddr::V4(ip) if ip.is_private()),
        "Web administration must bind to a loopback IP or an explicit RFC1918 private IPv4 address; wildcard and other addresses are not allowed."
    );
    Ok(())
}

async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_accepts_loopback_and_rfc1918_addresses() {
        for address in [
            "127.0.0.1:8787",
            "127.0.0.2:0",
            "[::1]:8787",
            "10.0.0.1:8787",
            "10.255.255.254:8787",
            "172.16.0.1:8787",
            "172.31.255.254:8787",
            "192.168.0.1:8787",
            "192.168.1.20:8787",
            "192.168.255.254:8787",
        ] {
            assert!(
                validate_bind_address(address.parse().unwrap()).is_ok(),
                "{address}"
            );
        }
    }

    #[test]
    fn bind_rejects_wildcard_public_and_other_special_addresses() {
        for address in [
            "0.0.0.0:8787",
            "[::]:8787",
            "8.8.8.8:8787",
            "100.64.0.1:8787",
            "169.254.1.1:8787",
            "172.15.255.255:8787",
            "172.32.0.1:8787",
            "192.167.255.255:8787",
            "192.169.0.1:8787",
            "192.0.2.1:8787",
            "224.0.0.1:8787",
            "255.255.255.255:8787",
            "[2001:db8::1]:8787",
            "[fe80::1]:8787",
            "[fd00::1]:8787",
            "[::ffff:192.168.1.20]:8787",
            "[::ffff:127.0.0.1]:8787",
        ] {
            assert!(
                validate_bind_address(address.parse().unwrap()).is_err(),
                "{address}"
            );
        }
    }
}
