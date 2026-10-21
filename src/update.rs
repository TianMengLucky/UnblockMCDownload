use std::{
    error::Error,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

/// Checks or downloads a release without replacing the running executable.
pub fn self_update(version: Option<&str>) -> std::result::Result<String, String> {
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        include_str!("self-update.ps1"),
    ]);
    command.env(
        "UBMC_UPDATE_MODE",
        if version.is_some() {
            "download"
        } else {
            "check"
        },
    );
    if let Some(version) = version {
        if version.split('.').count() != 3
            || !version
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err("更新版本号无效。".into());
        }
        let directory = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("程序目录无效")?
            .to_owned();
        command.env("UBMC_UPDATE_VERSION", version).env(
            "UBMC_UPDATE_DESTINATION",
            directory.join(format!(".unblockmc-update-{}.exe", std::process::id())),
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if version.is_none() {
        let parse = |s: &str| {
            s.split('.')
                .map(str::parse::<u64>)
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        let latest = parse(&value).map_err(|_| "更新版本号无效")?;
        if latest.len() != 3 {
            return Err("更新版本号无效".into());
        }
        if latest <= parse(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())? {
            return Ok(String::new());
        }
    }
    Ok(value)
}

/// Starts a hidden installer which waits for this process to release its executable.
pub fn replace_and_restart(staged: &str) -> std::result::Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        include_str!("replace-update.ps1"),
    ]);
    command
        .env("UBMC_UPDATE_PID", std::process::id().to_string())
        .env("UBMC_UPDATE_TARGET", executable)
        .env("UBMC_UPDATE_STAGED", staged);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// Runs update commands without opening a window. No arguments starts the normal UI.
pub fn run_helper(args: &[OsString]) -> Option<std::result::Result<(), String>> {
    let first = args.first()?.to_str()?;
    if !matches!(first, "--patch-update" | "--update" | "update") {
        return None;
    }
    Some(run(args).map_err(|error| {
        let text = error.to_string();
        if let Some(installed) = args.get(2)
            && let Some(parent) = Path::new(installed).parent()
            && let Ok(mut log) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(parent.join("UnBlockMC.Update.log"))
        {
            let _ = writeln!(log, "{first}: {text}");
        }
        text
    }))
}

fn run(args: &[OsString]) -> Result<()> {
    if args.first().is_some_and(|s| s == "--patch-update") {
        if args.len() != 3 {
            return Err("更新修改参数无效。".into());
        }
        crate::unlock::unlock_download(Path::new(&args[1]), Path::new(&args[2]))?;
        return Ok(());
    }
    if args.len() != 5 {
        return Err("启动器更新参数无效。".into());
    }
    let installed = Path::new(&args[2]);
    let source = if args[0] == "update" {
        PathBuf::from(&args[3])
    } else {
        fs::canonicalize(installed)?
            .parent()
            .ok_or("启动器目录无效。")?
            .join("PCL")
            .join("Plain Craft Launcher 2.exe")
    };
    crate::unlock::unlock_download(&source, installed)?;
    let mut command = Command::new(&source);
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn()?;
    Ok(())
}
