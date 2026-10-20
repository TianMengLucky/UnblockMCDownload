use std::io::{Cursor, Read, Write};

use dotscope::assembly::InstructionAssembler;
use ristretto_classfile::{ClassAccessFlags, ClassFile, ConstantPool, JAVA_17, JAVA_21};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

#[test]
fn java_class_can_be_modified_and_repacked_in_a_jar() -> Result<(), Box<dyn std::error::Error>> {
    let mut constant_pool = ConstantPool::default();
    let this_class = constant_pool.add_class("Example")?;
    let super_class = constant_pool.add_class("java/lang/Object")?;
    let class = ClassFile {
        version: JAVA_21,
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        constant_pool,
        this_class,
        super_class,
        ..Default::default()
    };
    let mut original = Vec::new();
    class.to_bytes(&mut original)?;

    let mut modified = ClassFile::from_bytes(&original)?;
    modified.version = JAVA_17;
    modified.verify()?;
    let mut bytes = Vec::new();
    modified.to_bytes(&mut bytes)?;

    let mut jar = ZipWriter::new(Cursor::new(Vec::new()));
    jar.start_file(
        "Example.class",
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )?;
    jar.write_all(&bytes)?;
    let mut jar = ZipArchive::new(jar.finish()?)?;
    let mut extracted = Vec::new();
    jar.by_name("Example.class")?.read_to_end(&mut extracted)?;

    let reloaded = ClassFile::from_bytes(&extracted)?;
    assert_eq!(reloaded.version, JAVA_17);
    assert_eq!(extracted, bytes);
    assert_ne!(extracted, original);
    Ok(())
}

#[test]
fn dotnet_method_body_can_be_encoded() -> dotscope::Result<()> {
    let mut assembler = InstructionAssembler::new();
    assembler.ldc_i4_s(42)?.ret()?;
    let (bytecode, max_stack, handlers) = assembler.finish()?;

    // CIL: ldc.i4.s 42; ret
    assert_eq!(bytecode, [0x1f, 42, 0x2a]);
    assert_eq!(max_stack, 1);
    assert!(handlers.is_empty());
    Ok(())
}
