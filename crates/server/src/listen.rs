//! Where the site listens: the Unix socket systemd hands over, or a TCP address for development.

use std::net::SocketAddr;

use listenfd::ListenFd;

/// A listening socket of either kind.
pub enum Listener {
    /// Handed over by systemd (`LISTEN_FDS`), as rtx-home's units do.
    Unix(tokio::net::UnixListener),
    /// Bound here.
    Tcp(tokio::net::TcpListener),
}

/// The socket systemd handed over, else `address`, bound now.
pub async fn listen(address: SocketAddr) -> Result<Listener, String> {
    let mut handed = ListenFd::from_env();
    if let Some(socket) = handed
        .take_unix_listener(0)
        .map_err(|e| format!("socket from systemd: {e}"))?
    {
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("socket from systemd: {e}"))?;
        let socket = tokio::net::UnixListener::from_std(socket)
            .map_err(|e| format!("socket from systemd: {e}"))?;
        return Ok(Listener::Unix(socket));
    }
    let socket = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|e| format!("{address}: {e}"))?;
    Ok(Listener::Tcp(socket))
}

/// Resolves when the site should stop: SIGTERM, or Ctrl-C.
pub async fn stopped() {
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!("cannot watch SIGTERM: {error}");
                std::future::pending::<()>().await;
            }
        }
    };
    tokio::select! {
        () = terminate => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    tracing::info!("stopping");
}
