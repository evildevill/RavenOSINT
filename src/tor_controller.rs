use std::time::Duration;

use tokio::net::{TcpStream, UnixStream};
use tokio::time::timeout;
use tracing::debug;

use crate::error::RavenError;

const TOR_TCP_CONTROL: &str = "127.0.0.1:9051";
const TOR_UNIX_CONTROL: &str = "/run/tor/control";

const TOR_COOKIE_PATHS: &[&str] = &[
    "/run/tor/control.authcookie",
    "/var/lib/tor/control_auth_cookie",
    "/usr/local/var/lib/tor/control_auth_cookie",
];

enum TorConnection {
    Tcp(TcpStream),
    Unix(UnixStream),
}

impl TorConnection {
    fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> impl std::future::Future<Output = std::io::Result<usize>> + 'a {
        async move {
            use tokio::io::AsyncReadExt;
            match self {
                TorConnection::Tcp(ref mut s) => AsyncReadExt::read(s, buf).await,
                TorConnection::Unix(ref mut s) => AsyncReadExt::read(s, buf).await,
            }
        }
    }

    fn write_all<'a>(&'a mut self, buf: &'a [u8]) -> impl std::future::Future<Output = std::io::Result<()>> + 'a {
        async move {
            use tokio::io::AsyncWriteExt;
            match self {
                TorConnection::Tcp(ref mut s) => AsyncWriteExt::write_all(s, buf).await,
                TorConnection::Unix(ref mut s) => AsyncWriteExt::write_all(s, buf).await,
            }
        }
    }
}

pub async fn new_tor_circuit() -> Result<(), RavenError> {
    let operation = async {
        let mut stream = connect_to_tor_control().await?;

        let auth_succeeded = try_cookie_auth(&mut stream).await
            || try_null_auth(&mut stream).await;

        if !auth_succeeded {
            return Err(RavenError::Other(
                "Tor authentication failed — ensure ControlPort (TCP 9051 or Unix socket) is enabled \
                 and authentication is configured (set CookieAuthentication 1 or \
                 HashedControlPassword in torrc)"
                    .to_string(),
            ));
        }

        send_command(&mut stream, "SIGNAL NEWNYM\r\n").await?;

        debug!("Tor circuit rotated successfully");
        tokio::time::sleep(Duration::from_secs(2)).await;

        Ok(())
    };

    timeout(Duration::from_secs(10), operation).await
        .map_err(|_| RavenError::Other(
            "Tor circuit rotation timed out after 10s. Tor may need more time \
             to build a new circuit — try increasing --timeout or check Tor status."
                .to_string(),
        ))?
}

async fn connect_to_tor_control() -> Result<TorConnection, RavenError> {
    if let Ok(unix) = UnixStream::connect(TOR_UNIX_CONTROL).await {
        debug!("Connected to Tor control via Unix socket {TOR_UNIX_CONTROL}");
        return Ok(TorConnection::Unix(unix));
    }

    TcpStream::connect(TOR_TCP_CONTROL).await
        .map(TorConnection::Tcp)
        .map_err(|e| {
            RavenError::Other(format!(
                "Failed to connect to Tor control — tried Unix socket at {TOR_UNIX_CONTROL} \
                 and TCP at {TOR_TCP_CONTROL}: {e}. \
                 Ensure Tor is running with ControlPort enabled and your user has access \
                 (e.g. 'sudo usermod -a -G debian-tor $USER', then log out and back in)."
            ))
        })
}

async fn recv_line(stream: &mut TorConnection) -> Result<String, RavenError> {
    let mut buf = Vec::with_capacity(128);
    loop {
        let mut byte = [0u8; 1];
        let n = stream.read(&mut byte).await.map_err(|e| {
            RavenError::Other(format!("Tor read error: {e}"))
        })?;
        if n == 0 {
            return Err(RavenError::Other("Tor connection closed".to_string()));
        }
        if byte[0] == b'\n' {
            return Ok(String::from_utf8_lossy(&buf).trim().to_string());
        }
        buf.push(byte[0]);
    }
}

async fn send_command(stream: &mut TorConnection, cmd: &str) -> Result<String, RavenError> {
    stream.write_all(cmd.as_bytes()).await.map_err(|e| {
        RavenError::Other(format!("Tor write error: {e}"))
    })?;

    let response = recv_line(stream).await?;
    debug!("Tor response: {response}");

    if !response.starts_with("250") {
        return Err(RavenError::Other(format!("Tor command failed: {response}")));
    }

    Ok(response)
}

async fn try_cookie_auth(stream: &mut TorConnection) -> bool {
    for path in TOR_COOKIE_PATHS {
        if let Ok(cookie) = tokio::fs::read(path).await {
            if cookie.len() >= 32 {
                let hex: String = cookie[..32].iter().map(|b| format!("{b:02x}")).collect();
                debug!("Trying Tor cookie auth from {path}");
                let cmd = format!("AUTHENTICATE {hex}\r\n");
                match send_command(stream, &cmd).await {
                    Ok(_) => return true,
                    Err(e) => debug!("Cookie auth failed: {e}"),
                }
            }
        }
    }
    false
}

async fn try_null_auth(stream: &mut TorConnection) -> bool {
    debug!("Trying Tor null auth");
    match send_command(stream, "AUTHENTICATE\r\n").await {
        Ok(_) => true,
        Err(e) => {
            debug!("Null auth failed: {e}");
            false
        }
    }
}
