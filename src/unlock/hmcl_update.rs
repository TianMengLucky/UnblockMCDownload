use ristretto_classfile::{
    ClassFile, Constant, ConstantPool, Method, MethodAccessFlags,
    attributes::{Attribute, Instruction},
    byte_reader::ByteReader,
};

use super::Result;

pub(super) const HANDLER: &str = "org/jackhuang/hmcl/upgrade/UpdateHandler.class";
pub(super) const HELPER: &str = "org/jackhuang/hmcl/upgrade/UnBlockMcUpdate.class";
pub(super) const CHECKER: &str = "org/jackhuang/hmcl/upgrade/UpdateChecker.class";

pub(super) fn hook_checker(input: &[u8]) -> Result<Option<Vec<u8>>> {
    let class = ClassFile::from_bytes(input)?;
    if class.constant_pool.try_get_class(class.this_class)?
        != "org/jackhuang/hmcl/upgrade/UpdateChecker"
    {
        return Err("HMCL 更新检测类身份不匹配。".into());
    }
    let reference = method_ref(
        &class.constant_pool,
        "org/jackhuang/hmcl/upgrade/IntegrityChecker",
        "isSelfVerified",
        "()Z",
    )?
    .ok_or("HMCL 更新检测校验入口缺失。")?;
    let mut output = input.to_vec();
    let mut changes = 0;
    for (_, offset, length) in code_ranges(input)? {
        let mut reader = ByteReader::new(&input[offset..offset + length]);
        while reader.remaining() != 0 {
            let position = reader.position();
            if Instruction::from_bytes(&mut reader)? == Instruction::Invokestatic(reference) {
                output[offset + position..offset + position + 3]
                    .copy_from_slice(&[0x04, 0x00, 0x00]);
                changes += 1;
            }
        }
    }
    if changes > 1 {
        return Err("HMCL 更新检测校验流程不匹配。".into());
    }
    Ok((changes == 1).then_some(output))
}

pub(super) fn helper_class(helper_path: &str) -> Result<Vec<u8>> {
    let mut class = ClassFile::from_bytes(include_bytes!("patches/UnBlockMcUpdate.class"))?;
    let mut replaced = false;
    for i in 1..=u16::try_from(class.constant_pool.len())? {
        if class
            .constant_pool
            .try_get_utf8(i)
            .is_ok_and(|value| value == "__UNBLOCKMC_HELPER__")
        {
            class.constant_pool.set(i, Constant::utf8(helper_path))?;
            replaced = true;
        } else if class
            .constant_pool
            .try_get_utf8(i)
            .is_ok_and(|v| v == "__UNBLOCKMC_RELEASE__")
        {
            class
                .constant_pool
                .set(i, Constant::utf8(super::bootstrap::RELEASE))?;
        }
    }
    if !replaced {
        return Err("HMCL 更新助手模板损坏。".into());
    }
    let mut output = Vec::new();
    class.to_bytes(&mut output)?;
    Ok(output)
}

pub(super) fn hook_handler(input: &[u8]) -> Result<Option<Vec<u8>>> {
    const CLASS: &str = "org/jackhuang/hmcl/upgrade/UpdateHandler";
    const SIGNATURE: &str = "(Ljava/nio/file/Path;Ljava/nio/file/Path;)V";
    const ORIGINAL: &str = "unblockmc$requestUpdate";
    let mut class = ClassFile::from_bytes(input)?;
    if class.constant_pool.try_get_class(class.this_class)? != CLASS {
        return Err("HMCL 更新类身份不匹配。".into());
    }
    if class.methods.iter().any(|m| {
        class
            .constant_pool
            .try_get_utf8(m.name_index)
            .is_ok_and(|s| s == ORIGINAL)
    }) {
        return Ok(None);
    }
    let candidates = class
        .methods
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            (class.constant_pool.try_get_utf8(m.name_index).ok()? == "requestUpdate"
                && class.constant_pool.try_get_utf8(m.descriptor_index).ok()? == SIGNATURE)
                .then_some(i)
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err("HMCL 自动更新入口结构不匹配。".into());
    }
    let index = candidates[0];
    let original_name = class.constant_pool.add_utf8(ORIGINAL)?;
    let wrapper_name = class.methods[index].name_index;
    let descriptor_index = class.methods[index].descriptor_index;
    class.methods[index].name_index = original_name;
    let pool = &mut class.constant_pool;
    let checker = pool.add_class("org/jackhuang/hmcl/upgrade/IntegrityChecker")?;
    let verify = pool.add_method_ref(checker, "verifyJar", "(Ljava/nio/file/Path;)V")?;
    let bridge = pool.add_class("org/jackhuang/hmcl/upgrade/UnBlockMcUpdate")?;
    let prepare = pool.add_method_ref(bridge, "prepare", SIGNATURE)?;
    let original = pool.add_method_ref(class.this_class, ORIGINAL, SIGNATURE)?;
    let code_name = pool.add_utf8("Code")?;
    class.methods.push(Method {
        access_flags: MethodAccessFlags::PRIVATE | MethodAccessFlags::STATIC,
        name_index: wrapper_name,
        descriptor_index,
        attributes: vec![Attribute::Code {
            name_index: code_name,
            max_stack: 2,
            max_locals: 2,
            code: vec![
                Instruction::Aload_0,
                Instruction::Invokestatic(verify),
                Instruction::Aload_0,
                Instruction::Aload_1,
                Instruction::Invokestatic(prepare),
                Instruction::Aload_0,
                Instruction::Aload_1,
                Instruction::Invokestatic(original),
                Instruction::Return,
            ],
            exception_table: vec![],
            attributes: vec![],
        }],
    });
    let mut output = Vec::new();
    class.to_bytes(&mut output)?;
    let pool = &class.constant_pool;
    let self_verify = method_ref(
        pool,
        "org/jackhuang/hmcl/upgrade/IntegrityChecker",
        "isSelfVerified",
        "()Z",
    )?
    .ok_or("HMCL 自身更新校验入口缺失。")?;
    let verify = method_ref(
        pool,
        "org/jackhuang/hmcl/upgrade/IntegrityChecker",
        "verifyJar",
        "(Ljava/nio/file/Path;)V",
    )?
    .ok_or("HMCL 下载签名校验入口缺失。")?;
    let ranges = code_ranges(&output)?;
    let mut self_checks = 0;
    let mut moved_verification = 0;
    for (name_index, offset, length) in ranges {
        let name = pool.try_get_utf8(name_index)?;
        let mut reader = ByteReader::new(&output[offset..offset + length]);
        let mut edits = Vec::new();
        while reader.remaining() != 0 {
            let position = reader.position();
            let instruction = Instruction::from_bytes(&mut reader)?;
            match instruction {
                Instruction::Invokestatic(reference) if reference == self_verify => {
                    // Permit locally modified launchers to update; downloaded originals are verified below.
                    edits.push((position, [0x04, 0x00, 0x00]));
                    self_checks += 1;
                }
                Instruction::Invokestatic(reference) if reference == verify && name == ORIGINAL => {
                    // The wrapper verifies the official file before invoking the patch helper.
                    edits.push((position, [0x57, 0x00, 0x00]));
                    moved_verification += 1;
                }
                _ => {}
            }
        }
        for (position, bytes) in edits {
            output[offset + position..offset + position + 3].copy_from_slice(&bytes);
        }
    }
    if self_checks < 2 || moved_verification != 1 {
        return Err("HMCL 自动更新校验流程不匹配。".into());
    }
    ClassFile::from_bytes(&output)?.verify()?;
    Ok(Some(output))
}

fn method_ref(
    pool: &ConstantPool,
    class: &str,
    name: &str,
    signature: &str,
) -> Result<Option<u16>> {
    for i in 1..=u16::try_from(pool.len())? {
        if let Ok((class_index, name_and_type)) = pool.try_get_method_ref(i) {
            let (name_index, descriptor) = pool.try_get_name_and_type(*name_and_type)?;
            if pool.try_get_class(*class_index)? == class
                && pool.try_get_utf8(*name_index)? == name
                && pool.try_get_utf8(*descriptor)? == signature
            {
                return Ok(Some(i));
            }
        }
    }
    Ok(None)
}

// Locate bytecode without rewriting branch offsets or existing stack-map frames.
fn code_ranges(input: &[u8]) -> Result<Vec<(u16, usize, usize)>> {
    let mut reader = ByteReader::new(input);
    reader.skip(8)?;
    let pool = ConstantPool::from_bytes(&mut reader)?;
    reader.skip(6)?;
    let interfaces = reader.read_u16()? as usize;
    reader.skip(interfaces * 2)?;
    let fields = reader.read_u16()?;
    for _ in 0..fields {
        reader.skip(6)?;
        skip_attributes(&mut reader)?;
    }
    let methods = reader.read_u16()?;
    let mut result = Vec::new();
    for _ in 0..methods {
        reader.skip(2)?;
        let name = reader.read_u16()?;
        reader.skip(2)?;
        let attributes = reader.read_u16()?;
        for _ in 0..attributes {
            let attribute = reader.read_u16()?;
            let length = reader.read_u32()? as usize;
            let start = reader.position();
            if pool.try_get_utf8(attribute)? == "Code" {
                let mut code = ByteReader::new(reader.remaining_slice());
                code.skip(4)?;
                let code_length = code.read_u32()? as usize;
                if code_length + 8 > length {
                    return Err("HMCL 更新方法体长度无效。".into());
                }
                result.push((name, start + 8, code_length));
            }
            reader.skip(length)?;
        }
    }
    Ok(result)
}

fn skip_attributes(reader: &mut ByteReader) -> Result<()> {
    for _ in 0..reader.read_u16()? {
        reader.skip(2)?;
        let length = reader.read_u32()? as usize;
        reader.skip(length)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(handler: bool) -> Vec<u8> {
        let mut class = ClassFile {
            version: ristretto_classfile::JAVA_17,
            ..Default::default()
        };
        class.this_class = class
            .constant_pool
            .add_class(if handler {
                "org/jackhuang/hmcl/upgrade/UpdateHandler"
            } else {
                "org/jackhuang/hmcl/upgrade/UpdateChecker"
            })
            .unwrap();
        class.super_class = class.constant_pool.add_class("java/lang/Object").unwrap();
        let pool = &mut class.constant_pool;
        let checker = pool
            .add_class("org/jackhuang/hmcl/upgrade/IntegrityChecker")
            .unwrap();
        let verify = pool
            .add_method_ref(checker, "verifyJar", "(Ljava/nio/file/Path;)V")
            .unwrap();
        let self_verify = pool
            .add_method_ref(checker, "isSelfVerified", "()Z")
            .unwrap();
        let code_name = pool.add_utf8("Code").unwrap();
        if handler {
            class.methods.push(Method {
                access_flags: MethodAccessFlags::PRIVATE | MethodAccessFlags::STATIC,
                name_index: pool.add_utf8("requestUpdate").unwrap(),
                descriptor_index: pool
                    .add_utf8("(Ljava/nio/file/Path;Ljava/nio/file/Path;)V")
                    .unwrap(),
                attributes: vec![Attribute::Code {
                    name_index: code_name,
                    max_stack: 1,
                    max_locals: 2,
                    code: vec![
                        Instruction::Aload_0,
                        Instruction::Invokestatic(verify),
                        Instruction::Return,
                    ],
                    exception_table: vec![],
                    attributes: vec![],
                }],
            });
        }
        let mut code = vec![Instruction::Invokestatic(self_verify), Instruction::Pop];
        if handler {
            code.extend([Instruction::Invokestatic(self_verify), Instruction::Pop]);
        }
        code.push(Instruction::Return);
        class.methods.push(Method {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
            name_index: pool.add_utf8("check").unwrap(),
            descriptor_index: pool.add_utf8("()V").unwrap(),
            attributes: vec![Attribute::Code {
                name_index: code_name,
                max_stack: 1,
                max_locals: 0,
                code,
                exception_table: vec![],
                attributes: vec![],
            }],
        });
        let mut bytes = Vec::new();
        class.to_bytes(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn downloaded_original_is_verified_before_patch_and_install() {
        let patched = hook_handler(&fixture(true)).unwrap().unwrap();
        let class = ClassFile::from_bytes(&patched).unwrap();
        class.verify().unwrap();
        let wrapper = class.methods.last().unwrap();
        let Attribute::Code { code, .. } = &wrapper.attributes[0] else {
            panic!("missing code")
        };
        let pool = &class.constant_pool;
        let verify = method_ref(
            pool,
            "org/jackhuang/hmcl/upgrade/IntegrityChecker",
            "verifyJar",
            "(Ljava/nio/file/Path;)V",
        )
        .unwrap()
        .unwrap();
        let prepare = method_ref(
            pool,
            "org/jackhuang/hmcl/upgrade/UnBlockMcUpdate",
            "prepare",
            "(Ljava/nio/file/Path;Ljava/nio/file/Path;)V",
        )
        .unwrap()
        .unwrap();
        let Instruction::Invokestatic(wrapper_verify) = code[1] else {
            panic!("verification must precede patching")
        };
        let (owner, name_and_type) = pool.try_get_method_ref(wrapper_verify).unwrap();
        let (name, _) = pool.try_get_name_and_type(*name_and_type).unwrap();
        assert_eq!(
            pool.try_get_class(*owner).unwrap(),
            "org/jackhuang/hmcl/upgrade/IntegrityChecker"
        );
        assert_eq!(pool.try_get_utf8(*name).unwrap(), "verifyJar");
        assert_eq!(code[4], Instruction::Invokestatic(prepare));
        let Attribute::Code { code, .. } = &class.methods[0].attributes[0] else {
            panic!("missing original")
        };
        assert!(!code.contains(&Instruction::Invokestatic(verify)));
        assert!(hook_handler(&patched).unwrap().is_none());
        let bridge = helper_class(r"C:\测试\helper.exe").unwrap();
        ClassFile::from_bytes(&bridge).unwrap().verify().unwrap();
    }

    #[test]
    fn modified_launcher_can_check_updates_without_changing_integrity_checker() {
        let patched = hook_checker(&fixture(false)).unwrap().unwrap();
        ClassFile::from_bytes(&patched).unwrap().verify().unwrap();
        assert!(hook_checker(&patched).unwrap().is_none());
    }

    #[test]
    fn region_and_update_hooks_share_one_archive_and_preserve_backup() {
        use std::io::{Cursor, Read, Write};
        use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

        let mut region = ClassFile::from_bytes(&fixture(false)).unwrap();
        region.this_class = region
            .constant_pool
            .add_class("org/jackhuang/hmcl/util/RegionUtils")
            .unwrap();
        region.methods.clear();
        region.methods.push(Method {
            access_flags: MethodAccessFlags::PRIVATE | MethodAccessFlags::STATIC,
            name_index: region
                .constant_pool
                .add_utf8("checkChinaMainlandNetwork")
                .unwrap(),
            descriptor_index: region.constant_pool.add_utf8("()Z").unwrap(),
            attributes: vec![Attribute::Code {
                name_index: region.constant_pool.add_utf8("Code").unwrap(),
                max_stack: 1,
                max_locals: 0,
                code: vec![Instruction::Iconst_1, Instruction::Ireturn],
                exception_table: vec![],
                attributes: vec![],
            }],
        });
        let mut bytes = Vec::new();
        region.to_bytes(&mut bytes).unwrap();
        let mut prefix = vec![0x55; 128];
        prefix[..2].copy_from_slice(b"MZ");
        let mut cursor = Cursor::new(prefix);
        cursor.set_position(128);
        let mut writer = ZipWriter::new(cursor);
        for (name, bytes) in [
            (super::super::REGION_CLASS, bytes),
            (HANDLER, fixture(true)),
            (CHECKER, fixture(false)),
            ("assets/unchanged.txt", b"unchanged resource".to_vec()),
        ] {
            writer
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        let input = writer.finish().unwrap().into_inner();
        for unlock_first in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("launcher.exe");
            std::fs::write(&path, &input).unwrap();
            if unlock_first {
                super::super::unlock(&path).unwrap();
            }
            let options = super::super::UnlockOptions {
                modify_auto_update: true,
            };
            let outcome = super::super::unlock_with_options(&path, options).unwrap();
            assert_eq!(std::fs::read(outcome.backup.unwrap()).unwrap(), input);
            let patched = std::fs::read(&path).unwrap();
            assert_eq!(&patched[..128], &input[..128]);
            let mut archive = ZipArchive::new(Cursor::new(&patched)).unwrap();
            for name in [super::super::REGION_CLASS, HANDLER, CHECKER, HELPER] {
                let mut class = Vec::new();
                archive
                    .by_name(name)
                    .unwrap()
                    .read_to_end(&mut class)
                    .unwrap();
                ClassFile::from_bytes(&class).unwrap().verify().unwrap();
                if name == super::super::REGION_CLASS {
                    assert!(super::super::patch_region_class(&class).unwrap().is_none());
                }
            }
            assert!(
                super::super::unlock_with_options(&path, options)
                    .unwrap()
                    .backup
                    .is_none()
            );
            assert_eq!(std::fs::read(&path).unwrap(), patched);
        }
    }
}
