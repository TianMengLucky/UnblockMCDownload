use std::{
    fs,
    io::{Cursor, Read, Write},
};

use ristretto_classfile::{
    ClassAccessFlags, ClassFile, ConstantPool, JAVA_17, Method, MethodAccessFlags,
    attributes::{Attribute, Instruction},
};
use unblockmcdownload::unlock::{Launcher, UnlockOptions, unlock, unlock_with_options};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

const REGION_CLASS: &str = "org/jackhuang/hmcl/util/RegionUtils.class";

fn hmcl_fixture(method_name: &str, signed: bool) -> Vec<u8> {
    let mut pool = ConstantPool::default();
    let this_class = pool
        .add_class("org/jackhuang/hmcl/util/RegionUtils")
        .unwrap();
    let super_class = pool.add_class("java/lang/Object").unwrap();
    let name_index = pool.add_utf8(method_name).unwrap();
    let descriptor_index = pool.add_utf8("()Z").unwrap();
    let code_index = pool.add_utf8("Code").unwrap();
    let method = Method {
        access_flags: MethodAccessFlags::PRIVATE | MethodAccessFlags::STATIC,
        name_index,
        descriptor_index,
        attributes: vec![Attribute::Code {
            name_index: code_index,
            max_stack: 1,
            max_locals: 0,
            code: vec![Instruction::Iconst_1, Instruction::Ireturn],
            exception_table: vec![],
            attributes: vec![],
        }],
    };
    let class = ClassFile {
        version: JAVA_17,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        constant_pool: pool,
        this_class,
        super_class,
        methods: vec![method],
        ..Default::default()
    };
    let mut bytes = Vec::new();
    class.to_bytes(&mut bytes).unwrap();
    let mut prefix = vec![0x55; 128];
    prefix[..2].copy_from_slice(b"MZ");
    let mut cursor = Cursor::new(prefix);
    cursor.set_position(128);
    let mut jar = ZipWriter::new(cursor);
    jar.set_comment("preserved comment").unwrap();
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    jar.start_file(REGION_CLASS, options).unwrap();
    jar.write_all(&bytes).unwrap();
    jar.start_file("assets/unchanged.txt", options).unwrap();
    jar.write_all(b"resource data").unwrap();
    if signed {
        jar.start_file("META-INF/TEST.SF", options).unwrap();
        jar.write_all(b"Signature-Version: 1.0").unwrap();
    }
    jar.finish().unwrap().into_inner()
}

#[test]
fn hmcl_patch_preserves_host_resources_backup_and_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("重命名的启动器.EXE");
    let original = hmcl_fixture("checkChinaMainlandNetwork", false);
    fs::write(&path, &original).unwrap();
    let result = unlock(&path).unwrap();
    assert_eq!(result.launcher, Launcher::Hmcl);
    assert_eq!(fs::read(result.backup.unwrap()).unwrap(), original);
    let patched = fs::read(&path).unwrap();
    assert_eq!(&patched[..128], &original[..128]);
    let mut jar = ZipArchive::new(Cursor::new(&patched)).unwrap();
    assert_eq!(jar.comment(), b"preserved comment");
    let mut resource = String::new();
    jar.by_name("assets/unchanged.txt")
        .unwrap()
        .read_to_string(&mut resource)
        .unwrap();
    assert_eq!(resource, "resource data");
    let mut bytes = Vec::new();
    jar.by_name(REGION_CLASS)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let class = ClassFile::from_bytes(&bytes).unwrap();
    class.verify().unwrap();
    let Attribute::Code {
        code,
        max_stack,
        max_locals,
        attributes,
        exception_table,
        ..
    } = &class.methods[0].attributes[0]
    else {
        panic!("missing code")
    };
    assert_eq!(code, &[Instruction::Iconst_0, Instruction::Ireturn]);
    assert_eq!((*max_stack, *max_locals), (1, 0));
    assert!(attributes.is_empty() && exception_table.is_empty());
    assert!(unlock(&path).unwrap().backup.is_none());
    assert_eq!(fs::read(&path).unwrap(), patched);
}

#[test]
fn existing_backup_is_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("launcher.exe");
    let backup = directory.path().join("launcher.exe.bak");
    let original = hmcl_fixture("checkChinaMainlandNetwork", false);
    fs::write(&path, &original).unwrap();
    fs::write(&backup, b"keep this backup").unwrap();
    assert!(
        unlock(&path)
            .unwrap_err()
            .to_string()
            .contains("已有备份不会覆盖")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(&backup).unwrap(), b"keep this backup");
}

#[test]
fn unknown_or_signed_files_remain_untouched() {
    for bytes in [
        b"not an executable".to_vec(),
        vec![b'M', b'Z'],
        hmcl_fixture("changedMethodName", false),
        hmcl_fixture("checkChinaMainlandNetwork", true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("launcher.exe");
        fs::write(&path, &bytes).unwrap();
        assert!(unlock(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}

#[test]
fn wrong_extensions_and_directories_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("launcher.txt");
    fs::write(&path, b"MZ").unwrap();
    assert!(unlock(&path).is_err());
    assert!(unlock(directory.path()).is_err());
}

#[cfg(windows)]
#[test]
fn an_open_executable_is_not_modified() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("launcher.exe");
    let original = hmcl_fixture("checkChinaMainlandNetwork", false);
    fs::write(&path, &original).unwrap();
    let _held_open = fs::File::open(&path).unwrap();
    assert!(unlock(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn unsupported_update_flow_leaves_launcher_and_directory_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("launcher.exe");
    let original = hmcl_fixture("checkChinaMainlandNetwork", false);
    fs::write(&path, &original).unwrap();
    assert!(
        unlock_with_options(
            &path,
            UnlockOptions {
                modify_auto_update: true
            }
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

fn update_hook_roundtrip(variable: &str, launcher: Launcher) {
    let input = fs::read(std::env::var_os(variable).unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("中文 launcher.exe");
    fs::write(&path, &input).unwrap();
    let backup = unlock(&path).unwrap().backup.unwrap();
    let base = fs::read(&path).unwrap();
    let options = UnlockOptions {
        modify_auto_update: true,
    };
    let result = unlock_with_options(&path, options).unwrap();
    assert_eq!(result.launcher, launcher);
    assert!(result.auto_update);
    let hooked = fs::read(&path).unwrap();
    assert_ne!(hooked, base);
    assert_eq!(fs::read(&backup).unwrap(), input);
    let helpers = fs::read_dir(directory.path())
        .unwrap()
        .filter_map(|e| {
            let e = e.unwrap();
            e.file_name()
                .to_string_lossy()
                .starts_with("UnBlockMC.Update-")
                .then_some(e.path())
        })
        .collect::<Vec<_>>();
    assert!(
        helpers.is_empty(),
        "must call this program without installing another executable"
    );
    assert!(
        unlock_with_options(&path, options)
            .unwrap()
            .backup
            .is_none()
    );
    assert_eq!(fs::read(&path).unwrap(), hooked);
    assert_eq!(fs::read(backup).unwrap(), input);
}

#[test]
#[ignore = "requires UNBLOCKMCDOWNLOAD_PCL_EXE"]
fn pcl_update_hook_preserves_existing_backup_and_is_idempotent() {
    update_hook_roundtrip("UNBLOCKMCDOWNLOAD_PCL_EXE", Launcher::Pcl);
}

#[test]
#[ignore = "requires UNBLOCKMCDOWNLOAD_HMCL_EXE"]
fn hmcl_update_hook_preserves_existing_backup_and_is_idempotent() {
    update_hook_roundtrip("UNBLOCKMCDOWNLOAD_HMCL_EXE", Launcher::Hmcl);
}

#[test]
#[ignore = "requires UNBLOCKMCDOWNLOAD_PCL_CE_EXE with UpdateManager.UpdateRestart"]
fn pcl_ce_update_hook_preserves_existing_backup_and_is_idempotent() {
    update_hook_roundtrip("UNBLOCKMCDOWNLOAD_PCL_CE_EXE", Launcher::PclCe);
}

#[test]
#[ignore = "requires UNBLOCKMCDOWNLOAD_CE_UPDATE_FIXTURE, built from tests/fixtures/ce"]
#[cfg(feature = "gui")]
fn updates_are_patched_without_the_ui_and_keep_the_next_update_hook() {
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    let original = std::env::var_os("UNBLOCKMCDOWNLOAD_CE_UPDATE_FIXTURE").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let installed = directory.path().join("已安装 $(1+2) & 启动器.exe");
    fs::copy(&original, &installed).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_unblockmcdownload"))
        .args([
            std::ffi::OsStr::new("--patch-update"),
            installed.as_os_str(),
            installed.as_os_str(),
        ])
        .status()
        .unwrap();
    assert!(
        result.success(),
        "helper failed: {:?}",
        fs::read_to_string(directory.path().join("UnBlockMC.Update.log"))
    );
    for _ in 1..=2 {
        let downloaded = directory.path().join("download.exe");
        fs::copy(&original, &downloaded).unwrap();
        let result = Command::new(&installed)
            .arg("simulate-update")
            .arg(&downloaded)
            .arg(&installed)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report = directory.path().join("update-result.txt");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !report.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let text = fs::read_to_string(&report).unwrap_or_else(|e| {
            panic!(
                "{e}; log: {:?}",
                fs::read_to_string(directory.path().join("UnBlockMC.Update.log"))
            )
        });
        assert!(
            text.contains("Mainland:False")
                && text.contains("Unknown:False")
                && text.contains("Other:False"),
            "{text}"
        );
        fs::remove_file(report).unwrap();
        // The next round runs the just-downloaded application, including its embedded hook.
        fs::copy(downloaded, &installed).unwrap();
    }
}

#[test]
#[ignore = "set UNBLOCKMCDOWNLOAD_PCL_EXE to the official PCL 2.99.0.0 executable"]
fn official_pcl_binary_roundtrip() {
    let input = fs::read(std::env::var_os("UNBLOCKMCDOWNLOAD_PCL_EXE").unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renamed.exe");
    fs::write(&path, &input).unwrap();
    let result = unlock(&path).unwrap();
    assert_eq!(result.launcher, Launcher::Pcl);
    assert_eq!(fs::read(result.backup.unwrap()).unwrap(), input);
    let output = fs::read(&path).unwrap();
    assert_ne!(input, output);
    use dotscope::metadata::{
        method::MethodBody,
        tables::{MethodDefRaw, TypeDefRaw},
    };
    let assembly = dotscope::CilAssemblyView::from_path(&path).unwrap();
    let identity = assembly.identity().unwrap().unwrap();
    assert!(identity.name.contains("Plain Craft Launcher"));
    let tables = assembly.tables().unwrap();
    let types = tables.table::<TypeDefRaw>().unwrap();
    let methods = tables.table::<MethodDefRaw>().unwrap();
    let strings = assembly.strings().unwrap();
    let mut restored = Vec::new();
    for row in types.iter() {
        let row = row.unwrap();
        let name = strings.get(row.type_name as usize).unwrap();
        if ![
            "ModModpack",
            "PageDownloadInstall",
            "PageDownloadModify",
            "PageDownloadResourceDetail",
        ]
        .contains(&name)
        {
            continue;
        }
        let end = types
            .get(row.rid + 1)
            .unwrap()
            .map_or(methods.row_count + 1, |r| r.method_list);
        for rid in row.method_list..end {
            let method = methods.get(rid).unwrap().unwrap();
            let method_name = strings.get(method.name as usize).unwrap();
            let expected = match (name, method_name) {
                ("ModModpack", "ModpackInstall") => &[123, 993][..],
                ("PageDownloadInstall", "LoaderInit") => &[51][..],
                ("PageDownloadModify", "LoadInstanceFinished") => &[1156][..],
                ("PageDownloadResourceDetail", "InitializeStackResourceVersions") => &[691][..],
                _ => continue,
            };
            let offset = assembly.file().rva_to_offset(method.rva as usize).unwrap();
            let body = MethodBody::from(&output[offset..]).unwrap();
            assert!(expected.contains(&body.size_code), "{name}.{method_name}");
            restored.push((name.to_owned(), method_name.to_owned()));
        }
    }
    assert_eq!(restored.len(), 5);
    drop(assembly);
    assert!(unlock(&path).unwrap().backup.is_none());
    assert_eq!(fs::read(&path).unwrap(), output);
}

#[test]
#[ignore = "set UNBLOCKMCDOWNLOAD_HMCL_EXE to an HMCL executable containing RegionUtils"]
fn official_hmcl_binary_roundtrip() {
    let input = fs::read(std::env::var_os("UNBLOCKMCDOWNLOAD_HMCL_EXE").unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renamed.exe");
    fs::write(&path, &input).unwrap();
    let result = unlock(&path).unwrap();
    assert_eq!(result.launcher, Launcher::Hmcl);
    assert_eq!(fs::read(result.backup.unwrap()).unwrap(), input);
    let output = fs::read(&path).unwrap();
    let mut before = ZipArchive::new(Cursor::new(&input)).unwrap();
    let mut after = ZipArchive::new(Cursor::new(&output)).unwrap();
    assert_eq!(before.len(), after.len());
    for i in 0..before.len() {
        let mut old = before.by_index(i).unwrap();
        let name = old.name().unwrap().into_owned();
        let mut new = after.by_name(&name).unwrap();
        let mut a = Vec::new();
        let mut b = Vec::new();
        old.read_to_end(&mut a).unwrap();
        new.read_to_end(&mut b).unwrap();
        if name == REGION_CLASS {
            assert_ne!(a, b);
        } else {
            assert_eq!(a, b, "changed {name}");
        }
    }
    assert!(unlock(&path).unwrap().backup.is_none());
}

#[test]
#[ignore = "set UNBLOCKMCDOWNLOAD_PCL_CE_EXE to a bundle containing the target commit's RegionPolicy"]
fn pcl_ce_bundle_roundtrip_and_policy_behavior() {
    let input_path = std::env::var_os("UNBLOCKMCDOWNLOAD_PCL_CE_EXE").unwrap();
    let input = fs::read(input_path).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renamed.exe");
    fs::write(&path, &input).unwrap();
    let result = unlock(&path).unwrap();
    assert_eq!(result.launcher, Launcher::PclCe);
    assert_eq!(fs::read(result.backup.unwrap()).unwrap(), input);
    assert!(unlock(&path).unwrap().backup.is_none());
    if std::env::var_os("UNBLOCKMCDOWNLOAD_CE_FIXTURE").is_some() {
        let result = std::process::Command::new(&path).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(text.contains("Mainland:False"), "{text}");
        assert!(text.contains("Unknown:False"), "{text}");
        assert!(text.contains("Other:False"), "{text}");
    }
}
