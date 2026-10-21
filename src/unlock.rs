use std::{
    error::Error,
    fmt,
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use ristretto_classfile::{
    ClassFile,
    attributes::{Attribute, Instruction},
};
use sha2::{Digest, Sha256};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

mod bootstrap;
mod hmcl_update;
mod managed;
mod pcl;
mod pcl_ce;
mod pcl_portable;

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const REGION_CLASS: &str = "org/jackhuang/hmcl/util/RegionUtils.class";
const MAX_FILE_SIZE: u64 = 256 * 1024 * 1024;
const MAX_CLASS_SIZE: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launcher {
    Pcl,
    Hmcl,
    PclCe,
}

impl fmt::Display for Launcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pcl => "PCL",
            Self::Hmcl => "HMCL",
            Self::PclCe => "PCL-CE",
        })
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub launcher: Launcher,
    pub backup: Option<PathBuf>,
    pub auto_update: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnlockOptions {
    pub modify_auto_update: bool,
}

/// Validates and stages the patch before creating a backup and replacing the executable.
pub fn unlock(path: &Path) -> Result<Outcome> {
    unlock_with_options(path, UnlockOptions::default())
}

pub fn unlock_with_options(path: &Path, options: UnlockOptions) -> Result<Outcome> {
    unlock_file(path, options, None)
}

pub(crate) fn unlock_download(path: &Path, installed: &Path) -> Result<Outcome> {
    unlock_file(
        path,
        UnlockOptions {
            modify_auto_update: true,
        },
        Some(installed),
    )
}

fn unlock_file(path: &Path, options: UnlockOptions, installed: Option<&Path>) -> Result<Outcome> {
    if installed.is_none()
        && !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err("请选择 PCL、HMCL 或 PCL-CE 的 .exe 文件。".into());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("请选择普通文件，不能选择目录或符号链接。".into());
    }
    if metadata.len() > MAX_FILE_SIZE {
        return Err("文件超过 256 MiB，无法作为支持的启动器处理。".into());
    }
    let target_path = fs::canonicalize(path)?;
    let installed_path = installed.map(fs::canonicalize).transpose()?;
    let is_download = installed_path.as_ref().is_some_and(|p| p != &target_path);
    let mut open_options = OpenOptions::new();
    open_options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Allow the final atomic replacement while excluding concurrent reads/writes.
        open_options.share_mode(0x0000_0004);
    }
    let mut original = open_options
        .open(path)
        .map_err(|err| format!("无法打开目标文件，请关闭启动器并检查写入权限：{err}"))?;
    let mut input = Vec::with_capacity(usize::try_from(metadata.len())?);
    (&mut original)
        .take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut input)?;
    if input.len() as u64 > MAX_FILE_SIZE {
        return Err("文件超过 256 MiB。".into());
    }
    let helper = options
        .modify_auto_update
        .then(std::env::current_exe)
        .transpose()?;
    let helper_text = helper
        .as_deref()
        .map(|path| path.to_str().ok_or("本程序路径包含无法表示的字符。"))
        .transpose()?;
    let (launcher, mut patched, already_unlocked) = patch_executable(&input, helper_text)?;
    if let Some(helper_text) = helper_text
        && launcher != Launcher::Hmcl
    {
        let bytes = patched.as_deref().unwrap_or(&input);
        let directory = installed_path
            .as_ref()
            .unwrap_or(&target_path)
            .parent()
            .and_then(Path::to_str)
            .ok_or("启动器目录无效。")?;
        let update = match launcher {
            Launcher::Pcl => managed::hook_update(bytes, "ModSecret", helper_text, directory)?,
            Launcher::PclCe => pcl_ce::hook_updates(
                bytes,
                pcl_ce::bundle_marker(bytes).ok_or("PCL-CE 单文件标记缺失。")?,
                helper_text,
                directory,
            )?,
            Launcher::Hmcl => unreachable!(),
        };
        if let Some(output) = update {
            patched = Some(output);
        }
    }
    let Some(patched) = patched else {
        return Ok(Outcome {
            launcher,
            backup: None,
            auto_update: options.modify_auto_update,
        });
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&patched)?;
    staged.as_file().sync_all()?;
    fs::set_permissions(staged.path(), metadata.permissions())?;

    let backup_path = if is_download {
        // These temporary files are downloaded again at the same path on subsequent updates.
        // Keep the installed launcher's original backup, and stage this download atomically.
        None
    } else {
        let mut backup_name = path.as_os_str().to_os_string();
        backup_name.push(".bak");
        let backup_path = PathBuf::from(backup_name);
        let backup = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backup_path);
        let mut backup = match backup {
            Ok(file) => Some(file),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && already_unlocked => {
                let previous = fs::read(&backup_path)?;
                let (kind, restored, _) = patch_executable(&previous, None)?;
                if kind != launcher || restored.as_deref().unwrap_or(&previous) != input {
                    return Err("已有备份与当前文件不匹配，未修改文件。".into());
                }
                None
            }
            Err(err) => {
                return Err({
                    format!(
                        "无法创建备份 {}（已有备份不会覆盖）：{err}",
                        backup_path.display()
                    )
                }
                .into());
            }
        };
        if let Some(file) = backup.as_mut()
            && let Err(err) = file.write_all(&input).and_then(|()| file.sync_all())
        {
            drop(backup);
            let _ = fs::remove_file(&backup_path);
            return Err(format!("备份失败，原文件未修改：{err}").into());
        }
        Some(backup_path)
    };
    // Release our handle before replacing the executable on Windows.
    drop(original);
    staged.persist(path).map_err(|err| match &backup_path {
        Some(backup) => format!("替换失败，备份保存在 {}：{err}", backup.display()),
        None => format!("替换下载文件失败：{err}"),
    })?;
    Ok(Outcome {
        launcher,
        backup: backup_path,
        auto_update: options.modify_auto_update,
    })
}

fn patch_executable(
    input: &[u8],
    hmcl_helper: Option<&str>,
) -> Result<(Launcher, Option<Vec<u8>>, bool)> {
    if !input.starts_with(b"MZ") && !input.starts_with(b"PK\x03\x04") {
        return Err("目标不是有效的 Windows EXE 文件。".into());
    }
    let hash: [u8; 32] = Sha256::digest(input).into();
    let result = |launcher, output: Option<Vec<u8>>| {
        let already_unlocked = output.is_none();
        (launcher, output, already_unlocked)
    };
    if pcl::recognizes(hash) {
        return pcl::patch_with_hash(input, hash).map(|output| result(Launcher::Pcl, output));
    }
    if let Some(marker) = pcl_ce::bundle_marker(input) {
        return pcl_ce::patch_bundle(input, marker).map(|output| result(Launcher::PclCe, output));
    }
    if let Ok(mut archive) = ZipArchive::new(Cursor::new(input)) {
        if archive.index_for_name(REGION_CLASS).is_some() {
            return patch_hmcl(input, &mut archive, hmcl_helper)
                .map(|(output, already_unlocked)| (Launcher::Hmcl, output, already_unlocked));
        }
        return Err("未找到 HMCL 的地区检测类；该版本不含此限制或暂不支持。".into());
    }
    if let Ok(view) = dotscope::CilAssemblyView::from_reader(Cursor::new(input))
        && view.identity()?.is_some_and(|identity| {
            identity.name.contains("Plain Craft Launcher") || identity.name == "PCL"
        })
    {
        return pcl_portable::patch(input).map(|output| result(Launcher::Pcl, output));
    }
    Err("无法识别为支持的 PCL、HMCL 或 PCL-CE 启动器；文件未修改。".into())
}

fn patch_region_class(input: &[u8]) -> Result<Option<Vec<u8>>> {
    let mut class = ClassFile::from_bytes(input)?;
    if class.constant_pool.try_get_class(class.this_class)? != "org/jackhuang/hmcl/util/RegionUtils"
    {
        return Err("HMCL 地区检测类名不匹配。".into());
    }
    let methods = class
        .methods
        .iter()
        .enumerate()
        .filter_map(|(index, method)| {
            (class.constant_pool.try_get_utf8(method.name_index).ok()?
                == "checkChinaMainlandNetwork"
                && class
                    .constant_pool
                    .try_get_utf8(method.descriptor_index)
                    .ok()?
                    == "()Z")
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if methods.len() != 1 {
        return Err("HMCL 地区检测方法不匹配，无法安全修改该版本。".into());
    }
    let method = &mut class.methods[methods[0]];
    if !method
        .access_flags
        .contains(ristretto_classfile::MethodAccessFlags::STATIC)
    {
        return Err("HMCL 地区检测方法签名不匹配。".into());
    }
    let code_attributes = method
        .attributes
        .iter_mut()
        .filter_map(|attribute| {
            if let Attribute::Code {
                max_stack,
                max_locals,
                code,
                exception_table,
                attributes,
                ..
            } = attribute
            {
                Some((max_stack, max_locals, code, exception_table, attributes))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if code_attributes.len() != 1 {
        return Err("HMCL 地区检测方法缺少唯一的代码体。".into());
    }
    let (max_stack, max_locals, code, exception_table, attributes) =
        code_attributes.into_iter().next().unwrap();
    if code == &[Instruction::Iconst_0, Instruction::Ireturn] && exception_table.is_empty() {
        return Ok(None);
    }
    *max_stack = 1;
    *max_locals = 0;
    *code = vec![Instruction::Iconst_0, Instruction::Ireturn];
    exception_table.clear();
    // Old stack-map frames and debug offsets no longer describe this method.
    attributes.clear();
    class.verify()?;
    let mut output = Vec::new();
    class.to_bytes(&mut output)?;
    Ok(Some(output))
}

fn patch_hmcl(
    input: &[u8],
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    helper: Option<&str>,
) -> Result<(Option<Vec<u8>>, bool)> {
    if archive.len() > 30_000 {
        return Err("HMCL 压缩包条目过多。".into());
    }
    let mut class = Vec::new();
    {
        let entry = archive.by_name(REGION_CLASS)?;
        if entry.size() > MAX_CLASS_SIZE {
            return Err("HMCL 地区检测类过大。".into());
        }
        entry.take(MAX_CLASS_SIZE + 1).read_to_end(&mut class)?;
        if class.len() as u64 > MAX_CLASS_SIZE {
            return Err("HMCL 地区检测类过大。".into());
        }
    }
    let patched = patch_region_class(&class)?;
    let already_unlocked = patched.is_none();
    let mut replacements = if let Some(helper) = helper {
        hmcl_update_replacements(archive, helper)?
    } else {
        std::collections::BTreeMap::new()
    };
    if let Some(patched) = patched {
        replacements.insert(REGION_CLASS.to_owned(), patched);
    }
    if replacements.is_empty() {
        return Ok((None, already_unlocked));
    }
    let output = repack_hmcl(input, archive, replacements)?;
    let mut reloaded = ZipArchive::new(Cursor::new(output.as_slice()))?;
    let mut class = Vec::new();
    reloaded.by_name(REGION_CLASS)?.read_to_end(&mut class)?;
    if patch_region_class(&class)?.is_some() {
        return Err("HMCL 修改后校验失败，原文件未修改。".into());
    }
    Ok((Some(output), already_unlocked))
}

fn hmcl_update_replacements(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    helper: &str,
) -> Result<std::collections::BTreeMap<String, Vec<u8>>> {
    let mut handler = Vec::new();
    archive
        .by_name(hmcl_update::HANDLER)?
        .take(MAX_CLASS_SIZE + 1)
        .read_to_end(&mut handler)?;
    if handler.len() as u64 > MAX_CLASS_SIZE {
        return Err("HMCL 更新类过大。".into());
    }
    let patched = hmcl_update::hook_handler(&handler)?;
    let mut checker = Vec::new();
    archive
        .by_name(hmcl_update::CHECKER)?
        .take(MAX_CLASS_SIZE + 1)
        .read_to_end(&mut checker)?;
    if checker.len() as u64 > MAX_CLASS_SIZE {
        return Err("HMCL 更新检测类过大。".into());
    }
    let checker = hmcl_update::hook_checker(&checker)?;
    let bridge = hmcl_update::helper_class(helper)?;
    let mut previous = Vec::new();
    if let Ok(entry) = archive.by_name(hmcl_update::HELPER) {
        entry.take(MAX_CLASS_SIZE + 1).read_to_end(&mut previous)?;
    }
    if patched.is_none() && checker.is_none() && previous == bridge {
        return Ok(std::collections::BTreeMap::new());
    }
    let mut replacements =
        std::collections::BTreeMap::from([(hmcl_update::HELPER.to_owned(), bridge)]);
    if let Some(bytes) = patched {
        replacements.insert(hmcl_update::HANDLER.to_owned(), bytes);
    }
    if let Some(bytes) = checker {
        replacements.insert(hmcl_update::CHECKER.to_owned(), bytes);
    }
    Ok(replacements)
}

fn repack_hmcl(
    input: &[u8],
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    mut replacements: std::collections::BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut prefix_len = input.len() as u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name()?.to_ascii_uppercase();
        if name.starts_with("META-INF/")
            && [".SF", ".RSA", ".DSA", ".EC"]
                .iter()
                .any(|ext| name.ends_with(ext))
        {
            return Err("此 HMCL 包含 JAR 数字签名，暂不支持修改。".into());
        }
        prefix_len = prefix_len.min(entry.header_start());
    }
    let prefix = input
        .get(..usize::try_from(prefix_len)?)
        .ok_or("HMCL EXE 前缀损坏。")?;
    if !prefix.is_empty() && prefix.len() < 64 {
        return Err("HMCL 缺少 Windows 启动器。".into());
    }
    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(prefix);
    let mut cursor = Cursor::new(output);
    cursor.set_position(prefix_len);
    let mut writer = ZipWriter::new(cursor);
    writer.set_raw_comment(archive.comment().into())?;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)?;
        let name = entry.name()?;
        if let Some(bytes) = replacements.remove(name.as_ref()) {
            writer.start_file(
                name,
                SimpleFileOptions::default().compression_method(entry.compression()),
            )?;
            writer.write_all(&bytes)?;
        } else {
            writer.raw_copy_file(entry)?;
        }
    }
    for (name, bytes) in replacements {
        writer.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        writer.write_all(&bytes)?;
    }
    let output = writer.finish()?.into_inner();
    if !output.starts_with(prefix) {
        return Err("HMCL 修改后校验失败，原文件未修改。".into());
    }
    Ok(output)
}
