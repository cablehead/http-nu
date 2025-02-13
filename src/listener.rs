use std::io::{self, Seek};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
use tokio_rustls::TlsAcceptor;

pub trait AsyncReadWrite: AsyncRead + AsyncWrite {}

impl<T: AsyncRead + AsyncWrite> AsyncReadWrite for T {}

pub type AsyncReadWriteBox = Box<dyn AsyncReadWrite + Unpin + Send>;

pub struct TlsConfig {
    acceptor: TlsAcceptor,
}

impl TlsConfig {
    pub fn from_pem(pem_path: PathBuf) -> io::Result<Self> {
        let pem = std::fs::File::open(pem_path)?;
        let mut pem = std::io::BufReader::new(pem);

        // Read certificates
        let mut certs = Vec::new();
        for cert in rustls_pemfile::certs(&mut pem) {
            certs.push(cert.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?);
        }

        // Reset reader to start
        pem.seek(std::io::SeekFrom::Start(0))?;

        // Read private key
        let key = rustls_pemfile::private_key(&mut pem)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "No private key found in PEM file",
                )
            })?;

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(config)),
        })
    }
}

pub enum Listener {
    Tcp {
        listener: TcpListener,
        tls_config: Option<TlsConfig>,
    },
    #[cfg(unix)]
    Unix(UnixListener),
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
                                format!("TLS error: {}", e),
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
        }
    }

    pub async fn bind(addr: &str, tls_config: Option<TlsConfig>) -> io::Result<Self> {
        #[cfg(windows)]
        {
            // On Windows, treat all addresses as TCP
            let mut addr = addr.to_owned();
            if addr.starts_with(':') {
                addr = format!("127.0.0.1{}", addr);
            }
            let listener = TcpListener::bind(addr).await?;
            Ok(Listener::Tcp {
                listener,
                tls_config,
            })
        }

        #[cfg(unix)]
        {
            if addr.starts_with('/') || addr.starts_with('.') {
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
                    addr = format!("127.0.0.1{}", addr);
                }
                let listener = TcpListener::bind(addr).await?;
                Ok(Listener::Tcp {
                    listener,
                    tls_config,
                })
            }
        }
    }

    #[allow(dead_code)]
    pub async fn connect(&self) -> io::Result<AsyncReadWriteBox> {
        match self {
            Listener::Tcp { listener, .. } => {
                let stream = TcpStream::connect(listener.local_addr()?).await?;
                Ok(Box::new(stream))
            }
            #[cfg(unix)]
            Listener::Unix(listener) => {
                let stream =
                    UnixStream::connect(listener.local_addr()?.as_pathname().unwrap()).await?;
                Ok(Box::new(stream))
            }
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
                write!(
                    f,
                    "{}:{} {}",
                    addr.ip(),
                    addr.port(),
                    if tls_config.is_some() { "(TLS)" } else { "" }
                )
            }
            #[cfg(unix)]
            Listener::Unix(listener) => {
                let addr = listener.local_addr().unwrap();
                let path = addr.as_pathname().unwrap();
                write!(f, "{}", path.display())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    async fn exercise_listener(addr: &str) {
        let mut listener = Listener::bind(addr).await.unwrap();
        let mut client = listener.connect().await.unwrap();

        let (mut serve, _) = listener.accept().await.unwrap();
        let want = b"Hello from server!";
        serve.write_all(want).await.unwrap();
        drop(serve);

        let mut got = Vec::new();
        client.read_to_end(&mut got).await.unwrap();
        assert_eq!(want.to_vec(), got);
    }

    #[tokio::test]
    async fn test_bind_tcp() {
        exercise_listener(":0").await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_bind_unix() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.sock");
        let path = path.to_str().unwrap();
        exercise_listener(path).await;
    }
}
