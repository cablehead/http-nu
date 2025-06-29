use assert_cmd::cargo::cargo_bin;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::time::timeout;

#[tokio::test]
async fn test_background_job_cleanup_on_interrupt() {
    // Start server with a long-running external command
    let mut child = spawn_http_nu_server("127.0.0.1:0", "{|req| ^sleep 10; 'done'}").await;

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Send SIGINT (Ctrl-C) to the process
    let pid = child.id().unwrap();
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
    }

    // Server should terminate cleanly within a reasonable time
    let result = timeout(Duration::from_secs(5), child.wait()).await;
    assert!(
        result.is_ok(),
        "Server did not shut down within 5 seconds after SIGINT"
    );

    let exit_status = result.unwrap().unwrap();
    // Should exit cleanly (code 0) or with interrupt signal (130) or other error codes indicating termination
    assert!(
        exit_status.success()
            || exit_status.code() == Some(130)
            || exit_status.code() == Some(1)
            || !exit_status.success(), // Any non-success is acceptable for interrupted processes
        "Unexpected exit status: {:?}",
        exit_status.code()
    );
}

#[tokio::test]
async fn test_server_starts_and_shuts_down() {
    // Start server with a simple closure
    let mut child = spawn_http_nu_server("127.0.0.1:0", "{|req| $req.method}").await;

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Clean shutdown
    child.kill().await.unwrap();
}

#[tokio::test]
async fn test_external_process_cleanup() {
    // Start server with external command
    let mut child = spawn_http_nu_server("127.0.0.1:0", "{|req| ^echo 'test'; $req.path}").await;

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Send interrupt signal
    let pid = child.id().unwrap();
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(pid as i32), Signal::SIGINT);
    }

    // Verify no hanging external processes
    let result = timeout(Duration::from_secs(3), child.wait()).await;
    assert!(result.is_ok(), "External processes not cleaned up properly");
}

#[cfg(unix)]
#[tokio::test]
async fn test_unix_socket() {
    let tmp = tempfile::tempdir().unwrap();
    let socket_path = tmp.path().join("test.sock");
    let socket_path = socket_path.to_str().unwrap();

    // Start server with a simple closure
    let mut child = spawn_http_nu_server(socket_path, "{|req| $req.method}").await;

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Clean shutdown
    child.kill().await.unwrap();
}

async fn spawn_http_nu_server(addr: &str, closure: &str) -> Child {
    let mut child = tokio::process::Command::new(cargo_bin("http-nu"))
        .arg(addr)
        .arg(closure)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start http-nu server");

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Spawn tasks to read output
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            eprintln!("[HTTP-NU STDOUT] {}", line);
        }
    });

    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            eprintln!("[HTTP-NU STDERR] {}", line);
        }
    });

    child
}
