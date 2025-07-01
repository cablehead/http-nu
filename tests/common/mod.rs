use assert_cmd::cargo::cargo_bin;
use std::process;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::time::timeout;

pub struct TestServer {
    pub child: Child,
    pub address: String,
}

impl TestServer {
    pub async fn new(addr: &str, closure: &str, tls: bool) -> Self {
        let mut cmd = tokio::process::Command::new(cargo_bin("http-nu"));
        cmd.arg(addr).arg(closure);

        if tls {
            cmd.arg("--tls").arg("tests/combined.pem");
        }

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to start http-nu server");

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

        // Spawn tasks to read output
        let mut addr_tx = Some(addr_tx);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("[HTTP-NU STDOUT] {line}");
                if let Some(tx) = addr_tx.take() {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(addr_str) = json.get("address").and_then(|a| a.as_str()) {
                            let _ = tx.send(addr_str.trim().to_string());
                        }
                    }
                }
            }
        });

        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                eprintln!("[HTTP-NU STDERR] {line}");
            }
        });

        let address = timeout(std::time::Duration::from_secs(5), addr_rx)
            .await
            .expect("Failed to get address from http-nu server")
            .expect("Channel closed before address received");

        Self { child, address }
    }

    pub async fn curl(&self, path: &str) -> process::Output {
        let url = if self.address.starts_with('/') {
            "http://localhost".to_string()
        } else {
            format!("http://{}", self.address)
        };

        let mut cmd = tokio::process::Command::new("curl");
        if self.address.starts_with('/') {
            cmd.arg("--unix-socket").arg(&self.address);
        }
        cmd.arg(format!("{url}{path}"));

        cmd.output().await.expect("Failed to execute curl")
    }

    pub async fn curl_tls(&self, path: &str) -> process::Output {
        // Extract port from address like "127.0.0.1:8080 (TLS)"
        let port = self.address
            .split_whitespace()
            .next()
            .unwrap()
            .split(':')
            .next_back()
            .unwrap();
        let mut cmd = tokio::process::Command::new("curl");
        cmd.arg("--cacert")
            .arg("tests/cert.pem")
            .arg("--resolve")
            .arg(format!("localhost:{port}:127.0.0.1"))
            .arg(format!("https://localhost:{port}{path}"));

        cmd.output().await.expect("Failed to execute curl")
    }

    #[cfg(unix)]
    pub fn send_ctrl_c(&self) {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(self.child.id().expect("child id") as i32);
        kill(pid, Signal::SIGINT).expect("failed to send SIGINT");
    }

    #[cfg(unix)]
    pub async fn wait_for_exit(&mut self) -> std::process::ExitStatus {
        use tokio::time::{timeout, Duration};
        timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("server did not exit in time")
            .expect("failed waiting for child")
    }

    pub fn has_exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if !self.has_exited() {
            let _ = self.child.start_kill();
        }
    }
}
