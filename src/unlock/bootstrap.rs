use base64::{Engine, engine::general_purpose::STANDARD};

pub(super) const RELEASE: &str =
    "https://github.com/TianMengLucky/UnblockMCDownload/releases/latest/download";

pub(super) fn command_prefix(tool: &str, directory: &str) -> String {
    command_prefix_with_release(tool, directory, RELEASE)
}

fn command_prefix_with_release(tool: &str, directory: &str, release: &str) -> String {
    let script = include_str!("patches/update-bootstrap.ps1")
        .replace("__UNBLOCKMC_TOOL__", &tool.replace('\'', "''"))
        .replace("__UNBLOCKMC_DIRECTORY__", &directory.replace('\'', "''"))
        .replace("__UNBLOCKMC_RELEASE__", &release.replace('\'', "''"));
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    // Only this fixed script is parsed as PowerShell. The launcher's original arguments
    // remain a comment and are recovered verbatim from the native command line.
    format!(
        "-NoProfile -NonInteractive -ExecutionPolicy Bypass -Command iex([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('{}'))) #UBMCARGS:",
        STANDARD.encode(bytes)
    )
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        os::windows::process::CommandExt,
        process::Command,
        time::{Duration, Instant},
    };

    fn download_case(valid_checksum: bool) {
        let directory = tempfile::tempdir().unwrap();
        let tool = directory.path().join("工具's $(1+2).exe");
        let bytes = fs::read(std::env::current_exe().unwrap()).unwrap();
        let expected = if valid_checksum {
            format!("{:x}", Sha256::digest(&bytes))
        } else {
            "0".repeat(64)
        };
        let checksum = format!("{expected} *解锁MC下载限制.exe\n");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let release = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut served = 0;
            while served < 2 && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let mut request = [0; 4096];
                        let n = stream.read(&mut request).unwrap();
                        let checksum_request =
                            String::from_utf8_lossy(&request[..n]).contains(".sha256");
                        let content = if checksum_request {
                            checksum.as_bytes()
                        } else {
                            &bytes
                        };
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            content.len()
                        )
                        .unwrap();
                        stream.write_all(content).unwrap();
                        served += 1;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            served
        });
        let powershell = std::path::Path::new(&std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let prefix = command_prefix_with_release(
            tool.to_str().unwrap(),
            directory.path().to_str().unwrap(),
            &release,
        );
        let output = Command::new(powershell)
            .raw_arg(format!("{prefix}--list"))
            .output()
            .unwrap();
        assert_eq!(
            server.join().unwrap(),
            2,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.status.success(),
            valid_checksum,
            "stderr: {}; log: {:?}",
            String::from_utf8_lossy(&output.stderr),
            fs::read_to_string(format!("{}.update.log", tool.display()))
        );
        assert_eq!(tool.is_file(), valid_checksum);
        if !valid_checksum {
            assert!(
                fs::read_to_string(format!("{}.update.log", tool.display()))
                    .unwrap()
                    .contains("failed SHA-256 verification")
            );
        }
        assert!(
            !fs::read_dir(directory.path()).unwrap().any(|e| e
                .unwrap()
                .path()
                .extension()
                .is_some_and(|e| e == "tmp"))
        );
    }

    #[test]
    fn missing_tool_is_downloaded_verified_and_run() {
        download_case(true);
    }

    #[test]
    fn incorrect_checksum_stops_before_executing_or_installing_tool() {
        download_case(false);
    }
}
