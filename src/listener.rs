use std::io::{self, Seek};
use std::path::PathBuf;
use std::sync::Arc;

use rustls::ServerConfig;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
#[cfg(unix)]
use tokio::net::UnixListener;
use tokio_rustls::TlsAcceptor;

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

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("TLS config error: {e}"))
            })?;

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
        }
    }

    pub async fn bind(addr: &str, tls_config: Option<TlsConfig>) -> io::Result<Self> {
        #[cfg(windows)]
        {
            // On Windows, treat all addresses as TCP
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

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
        };

        let client_task: tokio::task::JoinHandle<
            Result<Box<dyn AsyncReadWrite + Send + Unpin>, std::io::Error>,
        > = tokio::spawn(async move {
            if listener_addr.starts_with('/') {
                #[cfg(unix)]
                {
                    use tokio::net::UnixStream;
                    let stream = UnixStream::connect(&listener_addr).await?;
                    Ok(Box::new(stream) as AsyncReadWriteBox)
                }
                #[cfg(not(unix))]
                {
                    panic!("Unix sockets not supported on this platform");
                }
            } else {
                let stream = TcpStream::connect(&listener_addr).await?;
                Ok(Box::new(stream) as AsyncReadWriteBox)
            }
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
}
