use std::{
    error::Error,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

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
