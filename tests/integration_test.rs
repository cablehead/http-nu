use assert_cmd::cargo::cargo_bin;
use std::time::Duration;
use sysinfo::{Pid, System};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Child;
use tokio::time::timeout;

#[tokio::test]
async fn test_background_job_cleanup_on_interrupt() {
    // Start server with a long-running external command
    let mut child = spawn_http_nu_server("127.0.0.1:0", "{|req| ^sleep 99999; 'done'}").await;

    // Give server time to start and for the sleep command to start
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let server_pid = child.id().unwrap();
    let child_pids = get_child_pids(server_pid);
    assert!(
        !child_pids.is_empty(),
        "Expected at least one child process (the sleep command)"
    );

    // Send SIGINT (Ctrl-C) to the process
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid as NixPid;
        let _ = kill(NixPid::from_raw(server_pid as i32), Signal::SIGINT);
    }

    // Server should terminate cleanly within a reasonable time
    let result = timeout(Duration::from_secs(5), child.wait()).await;
    assert!(
        result.is_ok(),
        "Server did not shut down within 5 seconds after SIGINT"
    );

    // Verify all child processes have been terminated
    let mut sys = System::new();
    sys.refresh_all(); // Refresh process list again after parent has exited

    for pid_val in child_pids {
        let process_exists = sys.process(Pid::from_u32(pid_val)).is_some();
        assert!(
            !process_exists,
            "Child process {pid_val} should have been terminated"
        );
    }
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

    // Curl the socket to confirm the server is working
    let output = tokio::process::Command::new("curl")
        .arg("--unix-socket")
        .arg(socket_path)
        .arg("http://localhost")
        .output()
        .await
        .expect("Failed to execute curl");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "GET");

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
            eprintln!("[HTTP-NU STDOUT] {line}");
        }
    });

    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            eprintln!("[HTTP-NU STDERR] {line}");
        }
    });

    child
}

fn get_child_pids(target_pid_val: u32) -> Vec<u32> {
    let mut sys = System::new();
    sys.refresh_all();
    let target_sys_pid = Pid::from_u32(target_pid_val);
    sys.processes()
        .iter()
        .filter_map(|(pid, proc)| {
            // Check if this process's parent is our target
            match proc.parent() {
                Some(parent_pid) if parent_pid == target_sys_pid => Some(pid.as_u32()),
                _ => None,
            }
        })
        .collect()
}
