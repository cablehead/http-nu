use std::io::{self, Seek};
use std::path::PathBuf;
use std::sync::Arc;

use rustls::ServerConfig;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio_rustls::TlsAcceptor;

#[cfg(windows)]
mod win_uds_compat {
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tokio_util::compat::Compat;
    use win_uds::net::{AsyncListener, AsyncStream};

    pub struct WinUnixStream(Compat<AsyncStream>);

    impl WinUnixStream {
        pub async fn connect<P: AsRef<std::path::Path>>(path: P) -> io::Result<Self> {
            use tokio_util::compat::FuturesAsyncReadCompatExt;
            let stream = AsyncStream::connect(path).await?;
            Ok(Self(stream.compat()))
        }
    }

    impl AsyncRead for WinUnixStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.0).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for WinUnixStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.0).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.0).poll_flush(cx)
        }

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.0).poll_shutdown(cx)
        }
    }

    pub struct WinUnixListener {
        inner: AsyncListener,
        path: std::path::PathBuf,
    }

    impl WinUnixListener {
        pub fn bind<P: AsRef<std::path::Path>>(path: P) -> io::Result<Self> {
            let path_buf = path.as_ref().to_path_buf();
            Ok(Self {
                inner: AsyncListener::bind(path)?,
                path: path_buf,
            })
        }

        pub async fn accept(&self) -> io::Result<(WinUnixStream, ())> {
            use tokio_util::compat::FuturesAsyncReadCompatExt;
            let (stream, _addr) = self.inner.accept().await?;
            Ok((WinUnixStream(stream.compat()), ()))
        }

        pub fn local_addr(&self) -> io::Result<std::path::PathBuf> {
            Ok(self.path.clone())
        }
    }
}

#[cfg(windows)]
use win_uds_compat::WinUnixListener;

pub trait AsyncReadWrite: AsyncRead + AsyncWrite {}

impl<T: AsyncRead + AsyncWrite> AsyncReadWrite for T {}

pub type AsyncReadWriteBox = Box<dyn AsyncReadWrite + Unpin + Send>;

pub struct TlsConfig {
    pub config: Arc<ServerConfig>,
    acceptor: TlsAcceptor,
}

impl TlsConfig {
    pub fn from_pem(pem_path: PathBuf) -> io::Result<Self> {
        let pem = std::fs::File::open(&pem_path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Failed to open PEM file {}: {}", pem_path.display(), e),
            )
        })?;
        let mut pem = std::io::BufReader::new(pem);

        let certs = rustls_pemfile::certs(&mut pem)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid certificate: {e}"),
                )
            })?;

        if certs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "No certificates found",
            ));
        }

        pem.seek(std::io::SeekFrom::Start(0))?;

        let key = rustls_pemfile::private_key(&mut pem)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid private key: {e}"),
                )
            })?
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "No private key found"))?;

        let mut config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("TLS config error: {e}"))
            })?;

        // Enable HTTP/2 via ALPN (advertise h2 first, then http/1.1)
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let config = Arc::new(config);
        let acceptor = TlsAcceptor::from(config.clone());
        Ok(Self { config, acceptor })
    }
}

pub enum Listener {
    Tcp {
        listener: Arc<TcpListener>,
        tls_config: Option<TlsConfig>,
    },
    #[cfg(unix)]
    Unix(UnixListener),
    #[cfg(windows)]
    Unix(WinUnixListener),
    #[cfg(all(target_os = "linux", feature = "vsock"))]
    Vsock(tokio_vsock::VsockListener),
}

impl Listener {
    pub async fn accept(
        &mut self,
    ) -> io::Result<(AsyncReadWriteBox, Option<std::net::SocketAddr>)> {
        match self {
            Listener::Tcp {
                listener,
                tls_config,
            } => {
                let (stream, addr) = listener.accept().await?;

                let stream = if let Some(tls) = tls_config {
                    // Handle TLS connection
                    match tls.acceptor.accept(stream).await {
                        Ok(tls_stream) => Box::new(tls_stream) as AsyncReadWriteBox,
                        Err(e) => {
                            return Err(io::Error::new(
                                io::ErrorKind::ConnectionAborted,
                                format!("TLS error: {e}"),
                            ));
                        }
                    }
                } else {
                    // Handle plain TCP connection
                    Box::new(stream)
                };

                Ok((stream, Some(addr)))
            }
            #[cfg(unix)]
            Listener::Unix(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok((Box::new(stream), None))
            }
            #[cfg(windows)]
            Listener::Unix(listener) => {
                let (stream, _) = listener.accept().await?;
                Ok((Box::new(stream), None))
            }
            #[cfg(all(target_os = "linux", feature = "vsock"))]
            Listener::Vsock(listener) => {
                // A vsock peer has no IP, just like a Unix domain socket peer,
                // so report the address as None (see resolve_trusted_ip).
                let (stream, _peer) = listener.accept().await?;
                Ok((Box::new(stream), None))
            }
        }
    }

    pub async fn bind(addr: &str, tls_config: Option<TlsConfig>) -> io::Result<Self> {
        // vsock://<port> or vsock://<cid>:<port> selects an AF_VSOCK listener.
        if let Some(rest) = addr.strip_prefix("vsock://") {
            #[cfg(all(target_os = "linux", feature = "vsock"))]
            {
                if tls_config.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "TLS is not supported with vsock sockets",
                    ));
                }
                return Self::bind_vsock(rest);
            }
            #[cfg(not(all(target_os = "linux", feature = "vsock")))]
            {
                let _ = rest;
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "vsock support is not compiled in (Linux only, requires the \"vsock\" feature)",
                ));
            }
        }

        // Check if address looks like a Unix socket path
        fn is_unix_path(addr: &str) -> bool {
            addr.starts_with('/') || addr.starts_with('.')
        }

        #[cfg(windows)]
        fn is_windows_path(s: &str) -> bool {
            let bytes = s.as_bytes();
            bytes.len() >= 3
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && (bytes[2] == b'\\' || bytes[2] == b'/')
        }

        #[cfg(windows)]
        {
            if is_unix_path(addr) || is_windows_path(addr) {
                if tls_config.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "TLS is not supported with Unix domain sockets",
                    ));
                }
                let _ = std::fs::remove_file(addr);
                let listener = WinUnixListener::bind(addr)?;
                Ok(Listener::Unix(listener))
            } else {
                let mut addr = addr.to_owned();
                if addr.starts_with(':') {
                    addr = format!("127.0.0.1{addr}");
                }
                let listener = TcpListener::bind(addr).await?;
                Ok(Listener::Tcp {
                    listener: Arc::new(listener),
                    tls_config,
                })
            }
        }

        #[cfg(unix)]
        {
            if is_unix_path(addr) {
                if tls_config.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "TLS is not supported with Unix domain sockets",
                    ));
                }
                let _ = std::fs::remove_file(addr);
                let listener = UnixListener::bind(addr)?;
                Ok(Listener::Unix(listener))
            } else {
                let mut addr = addr.to_owned();
                if addr.starts_with(':') {
                    addr = format!("127.0.0.1{addr}");
                }
                let listener = TcpListener::bind(addr).await?;
                Ok(Listener::Tcp {
                    listener: Arc::new(listener),
                    tls_config,
                })
            }
        }
    }

    #[cfg(all(target_os = "linux", feature = "vsock"))]
    fn bind_vsock(rest: &str) -> io::Result<Self> {
        use tokio_vsock::{VsockAddr, VsockListener, VMADDR_CID_ANY};

        // Accept both "<port>" and "<cid>:<port>". Always bind to CID_ANY so the
        // listener works regardless of the guest's assigned CID; any CID given
        // on the listen side is accepted but ignored.
        let port_str = match rest.rsplit_once(':') {
            Some((_cid, port)) => port,
            None => rest,
        };
        let port: u32 = port_str.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid vsock port: {rest:?} (expected vsock://<port> or vsock://<cid>:<port>)"),
            )
        })?;
        let listener = VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, port))?;
        Ok(Listener::Vsock(listener))
    }
}

impl Clone for Listener {
    fn clone(&self) -> Self {
        match self {
            Listener::Tcp {
                listener,
                tls_config,
            } => Listener::Tcp {
                listener: listener.clone(),
                tls_config: tls_config.clone(),
            },
            #[cfg(unix)]
            Listener::Unix(_) => {
                panic!("Cannot clone a Unix listener")
            }
            #[cfg(windows)]
            Listener::Unix(_) => {
                panic!("Cannot clone a Unix listener")
            }
            #[cfg(all(target_os = "linux", feature = "vsock"))]
            Listener::Vsock(_) => {
                panic!("Cannot clone a vsock listener")
            }
        }
    }
}

impl Clone for TlsConfig {
    fn clone(&self) -> Self {
        TlsConfig {
            config: self.config.clone(),
            acceptor: TlsAcceptor::from(self.config.clone()),
        }
    }
}

impl std::fmt::Display for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Listener::Tcp {
                listener,
                tls_config,
            } => {
                let addr = listener.local_addr().unwrap();
                let tls_suffix = if tls_config.is_some() { " (TLS)" } else { "" };
                write!(f, "{}:{}{}", addr.ip(), addr.port(), tls_suffix)
            }
            #[cfg(unix)]
            Listener::Unix(listener) => {
                let addr = listener.local_addr().unwrap();
                let path = addr.as_pathname().unwrap();
                write!(f, "{}", path.display())
            }
            #[cfg(windows)]
            Listener::Unix(listener) => {
                let path = listener.local_addr().unwrap();
                write!(f, "{}", path.display())
            }
            #[cfg(all(target_os = "linux", feature = "vsock"))]
            Listener::Vsock(listener) => {
                let addr = listener.local_addr().unwrap();
                write!(f, "vsock://:{}", addr.port())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    #[cfg(windows)]
    use super::win_uds_compat::WinUnixStream;

    async fn exercise_listener(addr: &str) {
        let mut listener = Listener::bind(addr, None).await.unwrap();
        let listener_addr = match &listener {
            Listener::Tcp { listener, .. } => {
                let addr = listener.local_addr().unwrap();
                format!("{}:{}", addr.ip(), addr.port())
            }
            #[cfg(unix)]
            Listener::Unix(listener) => {
                let addr = listener.local_addr().unwrap();
                addr.as_pathname().unwrap().to_string_lossy().to_string()
            }
            #[cfg(windows)]
            Listener::Unix(listener) => {
                let path = listener.local_addr().unwrap();
                path.to_string_lossy().to_string()
            }
            #[cfg(all(target_os = "linux", feature = "vsock"))]
            Listener::Vsock(listener) => {
                let addr = listener.local_addr().unwrap();
                format!("vsock://:{}", addr.port())
            }
        };

        let client_task: tokio::task::JoinHandle<
            Result<Box<dyn AsyncReadWrite + Send + Unpin>, std::io::Error>,
        > = tokio::spawn(async move {
            #[cfg(unix)]
            if listener_addr.starts_with('/') {
                use tokio::net::UnixStream;
                let stream = UnixStream::connect(&listener_addr).await?;
                return Ok(Box::new(stream) as AsyncReadWriteBox);
            }
            #[cfg(windows)]
            if listener_addr.starts_with('/') || listener_addr.chars().nth(1) == Some(':') {
                let stream = WinUnixStream::connect(&listener_addr).await?;
                return Ok(Box::new(stream) as AsyncReadWriteBox);
            }
            #[cfg(all(target_os = "linux", feature = "vsock"))]
            if let Some(port) = listener_addr.strip_prefix("vsock://:") {
                use tokio_vsock::{VsockAddr, VsockStream, VMADDR_CID_LOCAL};
                let port: u32 = port.parse().unwrap();
                // Connect over the host-local CID (1) so the round-trip loops
                // back on this machine without a hypervisor.
                let stream = VsockStream::connect(VsockAddr::new(VMADDR_CID_LOCAL, port)).await?;
                return Ok(Box::new(stream) as AsyncReadWriteBox);
            }
            let stream = TcpStream::connect(&listener_addr).await?;
            Ok(Box::new(stream) as AsyncReadWriteBox)
        });

        let (mut serve, _) = listener.accept().await.unwrap();
        let want = b"Hello from server!";
        serve.write_all(want).await.unwrap();
        drop(serve);

        let mut client = client_task.await.unwrap().unwrap();
        let mut got = Vec::new();
        client.read_to_end(&mut got).await.unwrap();
        assert_eq!(want.to_vec(), got);
    }

    #[tokio::test]
    async fn test_bind_tcp() {
        exercise_listener("127.0.0.1:0").await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_bind_unix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.sock");
        let path = path.to_str().unwrap();
        exercise_listener(path).await;
    }

    #[cfg(all(target_os = "linux", feature = "vsock"))]
    #[tokio::test]
    async fn test_bind_vsock() {
        // Skip when the host has no vsock support (no /dev/vsock or no
        // vsock_loopback module) so CI without a hypervisor still passes.
        match Listener::bind("vsock://0", None).await {
            Ok(_probe) => {}
            Err(e) => {
                eprintln!("skipping test_bind_vsock: vsock unavailable: {e}");
                return;
            }
        }
        exercise_listener("vsock://0").await;
    }
}
