use std::{
    borrow::Cow,
    io::{Cursor, Read},
};

use dotscope::{
    CilAssemblyView,
    metadata::{
        method::MethodBody,
        tables::{MethodDefRaw, TypeDefRaw},
    },
};
use flate2::read::DeflateDecoder;
use sha2::{Digest, Sha256};

use super::Result;

// .NET apphost bundle marker, preceded by the 64-bit bundle manifest offset.
const MARKER: &[u8] = &[
    0x8b, 0x12, 0x02, 0xb9, 0x6a, 0x61, 0x20, 0x38, 0x72, 0x7b, 0x93, 0x02, 0x14, 0xd7, 0xa0, 0x32,
    0x13, 0xf5, 0xb9, 0xe6, 0xef, 0xae, 0x33, 0x18, 0xee, 0x3b, 0x2d, 0xce, 0x24, 0xb3, 0x6a, 0xae,
];
const MAX_ASSEMBLY_SIZE: usize = 32 * 1024 * 1024;

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(".NET 单文件偏移溢出。")?;
        let data = self
            .data
            .get(self.position..end)
            .ok_or(".NET 单文件数据不完整。")?;
        self.position = end;
        Ok(data)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn offset(&mut self) -> Result<usize> {
        Ok(usize::try_from(u64::from_le_bytes(
            self.take(8)?.try_into()?,
        ))?)
    }

    fn string(&mut self) -> Result<&'a [u8]> {
        let mut length = 0_u32;
        for shift in (0..35).step_by(7) {
            let byte = self.take(1)?[0];
            if shift == 28 && byte > 0x0f {
                return Err(".NET 单文件字符串长度溢出。".into());
            }
            length |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                if length > 4096 {
                    return Err(".NET 单文件字符串过长。".into());
                }
                return self.take(length as usize);
            }
        }
        Err(".NET 单文件字符串长度无效。".into())
    }
}

pub(super) fn bundle_marker(input: &[u8]) -> Option<usize> {
    memchr::memmem::find(input, MARKER)
}

pub(super) fn patch_bundle(input: &[u8], marker: usize) -> Result<Option<Vec<u8>>> {
    patch_component(input, marker, b"PCL.Core.dll", patch_core)
}

pub(super) fn hook_updates(
    input: &[u8],
    marker: usize,
    helper: &str,
    directory: &str,
) -> Result<Option<Vec<u8>>> {
    patch_component(input, marker, b"Plain Craft Launcher 2.dll", |bytes| {
        super::managed::hook_update(bytes, "UpdateManager", helper, directory)
    })
}

fn patch_component(
    input: &[u8],
    marker: usize,
    component: &[u8],
    patch: impl FnOnce(&[u8]) -> Result<Option<Vec<u8>>>,
) -> Result<Option<Vec<u8>>> {
    let mut pointer = Reader {
        data: input,
        position: marker.checked_sub(8).ok_or(".NET 单文件标记损坏。")?,
    };
    let header = pointer.offset()?;
    if header <= marker + MARKER.len() {
        return Err(".NET 单文件清单偏移无效。".into());
    }
    let mut reader = Reader {
        data: input,
        position: header,
    };
    let version = reader.u32()?;
    let minor = reader.u32()?;
    if !matches!(version, 2 | 6) || minor != 0 {
        return Err("暂不支持此 .NET 单文件格式。".into());
    }
    let count = reader.u32()?;
    if count == 0 || count > 10_000 {
        return Err(".NET 单文件条目数量无效。".into());
    }
    let id = reader.string()?;
    let id_position = reader.position - id.len();
    if id.len() != 12 {
        return Err(".NET 单文件缓存标识格式不支持。".into());
    }
    reader.take(40)?; // deps.json, runtimeconfig.json locations and bundle flags
    let mut target = None;
    let mut first_trailing_entry = input.len();
    for _ in 0..count {
        let entry_position = reader.position;
        let offset = reader.offset()?;
        let size = reader.offset()?;
        let compressed_size = if version >= 6 { reader.offset()? } else { 0 };
        let kind = reader.take(1)?[0];
        let name = reader.string()?;
        let stored_size = if compressed_size == 0 {
            size
        } else {
            compressed_size
        };
        let end = offset
            .checked_add(stored_size)
            .ok_or(".NET 单文件条目范围溢出。")?;
        if offset < 64 || end > input.len() || (offset < header && end > header) {
            return Err(".NET 单文件条目范围无效。".into());
        }
        if offset >= header {
            first_trailing_entry = first_trailing_entry.min(offset);
        }
        if name == component {
            if target.is_some() || kind != 1 || size > MAX_ASSEMBLY_SIZE {
                return Err("PCL-CE 核心程序集条目无效。".into());
            }
            target = Some((entry_position, offset, size, compressed_size, end));
        }
    }
    if first_trailing_entry < reader.position {
        return Err(".NET 单文件条目与清单重叠。".into());
    }
    let (entry_position, offset, size, compressed_size, end) =
        target.ok_or("此 .NET 单文件不含 PCL-CE 核心程序集。")?;
    let core = if compressed_size == 0 {
        Cow::Borrowed(&input[offset..end])
    } else {
        let mut core = Vec::with_capacity(size);
        DeflateDecoder::new(&input[offset..end])
            .take(size as u64 + 1)
            .read_to_end(&mut core)?;
        if core.len() != size {
            return Err("PCL-CE 核心程序集解压长度不匹配。".into());
        }
        Cow::Owned(core)
    };
    let Some(patched) = patch(&core)? else {
        return Ok(None);
    };
    drop(core);
    let in_place = compressed_size == 0 && patched.len() == size;
    let start = if in_place {
        offset
    } else {
        input.len().checked_add(4095).ok_or("PCL-CE 文件过大。")? & !4095
    };
    let output_len = if in_place {
        input.len()
    } else {
        let length = start
            .checked_add(patched.len())
            .ok_or("PCL-CE 文件过大。")?;
        if length > super::MAX_FILE_SIZE as usize {
            return Err("PCL-CE 修改后文件超过 256 MiB。".into());
        }
        length
    };
    let mut output = Vec::with_capacity(output_len);
    output.extend_from_slice(input);
    if in_place {
        output[offset..end].copy_from_slice(&patched);
    } else {
        // Keep every other bundle entry at its original offset; append an uncompressed replacement.
        output.resize(start, 0);
        output.extend_from_slice(&patched);
        output[entry_position..entry_position + 8].copy_from_slice(&(start as u64).to_le_bytes());
        output[entry_position + 8..entry_position + 16]
            .copy_from_slice(&(patched.len() as u64).to_le_bytes());
        if version >= 6 {
            output[entry_position + 16..entry_position + 24].fill(0);
        }
    }
    // Force a fresh extraction directory for bundles which extract managed assemblies.
    let mut cache_hash = Sha256::new();
    cache_hash.update(id);
    cache_hash.update(&patched);
    let digest = format!("{:x}", cache_hash.finalize());
    output[id_position..id_position + 12].copy_from_slice(&digest.as_bytes()[..12]);
    Ok(Some(output))
}

fn patch_core(input: &[u8]) -> Result<Option<Vec<u8>>> {
    let view = CilAssemblyView::from_reader(Cursor::new(input))?;
    if view.identity()?.is_none_or(|id| id.name != "PCL.Core") {
        return Err("PCL-CE 核心程序集身份不匹配。".into());
    }
    if view.cor20header().managed_native_header_size != 0 {
        return Err("暂不支持包含 ReadyToRun 本机代码的 PCL-CE。".into());
    }
    let tables = view.tables().ok_or("PCL-CE 缺少元数据表。")?;
    let types = tables.table::<TypeDefRaw>().ok_or("PCL-CE 缺少类型表。")?;
    let methods = tables
        .table::<MethodDefRaw>()
        .ok_or("PCL-CE 缺少方法表。")?;
    let strings = view.strings().ok_or("PCL-CE 缺少字符串表。")?;
    let blobs = view.blobs().ok_or("PCL-CE 缺少签名表。")?;
    let mut matches = Vec::new();
    for row in types.iter() {
        let row = row?;
        if strings.get(row.type_name as usize)? != "RegionAssessment"
            || strings.get(row.type_namespace as usize)? != "PCL.Core.App.Region"
        {
            continue;
        }
        let end = types
            .get(row.rid + 1)?
            .map_or(methods.row_count + 1, |next| next.method_list);
        for rid in row.method_list..end {
            let method = methods.get(rid)?.ok_or("PCL-CE 方法索引损坏。")?;
            if strings.get(method.name as usize)? == "get_IsRestricted" {
                if blobs.get(method.signature as usize)? != [0x20, 0, 2]
                    || method.impl_flags & 3 != 0
                    || method.rva == 0
                {
                    return Err("PCL-CE 限制判断的方法签名不匹配。".into());
                }
                matches.push(method.rva);
            }
        }
    }
    if matches.len() != 1 {
        return Err("已识别为 PCL-CE，但未找到指定提交的地区限制判断。".into());
    }
    let offset = view.file().rva_to_offset(matches[0] as usize)?;
    let bytes = input.get(offset..).ok_or("PCL-CE 方法偏移无效。")?;
    let body = MethodBody::from(bytes)?;
    if body.is_exception_data || body.size_code < 2 {
        return Err("PCL-CE 限制判断的代码结构不匹配。".into());
    }
    if bytes.starts_with(&[0x0a, 0x16, 0x2a]) {
        return Ok(None);
    }
    let mut output = if view.file().repairs().is_empty() {
        view.into_file()?.into_data()?
    } else {
        input.to_vec()
    };
    // Tiny CIL method: ldc.i4.0; ret. Keeping its RVA preserves all call sites.
    output
        .get_mut(offset..offset + 3)
        .ok_or("PCL-CE 方法体不完整。")?
        .copy_from_slice(&[0x0a, 0x16, 0x2a]);
    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn bundle(core: &[u8], compressed: bool, minimum_size: usize) -> (Vec<u8>, usize, usize) {
        let stored = if compressed {
            let mut encoder =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(core).unwrap();
            encoder.finish().unwrap()
        } else {
            core.to_vec()
        };
        let length = (64 + stored.len()).max(minimum_size);
        let mut input = Vec::with_capacity(length + 128);
        input.resize(length, 0);
        input[8..8 + MARKER.len()].copy_from_slice(MARKER);
        input[64..64 + stored.len()].copy_from_slice(&stored);
        let header = input.len();
        input[..8].copy_from_slice(&(header as u64).to_le_bytes());
        for value in [6_u32, 0, 1] {
            input.extend_from_slice(&value.to_le_bytes());
        }
        input.push(12);
        let id = input.len();
        input.extend_from_slice(b"original-id!");
        input.extend_from_slice(&[0; 40]);
        let entry = input.len();
        for value in [
            64,
            core.len() as u64,
            if compressed { stored.len() as u64 } else { 0 },
        ] {
            input.extend_from_slice(&value.to_le_bytes());
        }
        input.push(1);
        input.push(b"PCL.Core.dll".len() as u8);
        input.extend_from_slice(b"PCL.Core.dll");
        (input, entry, id)
    }

    #[test]
    fn marker_search_preserves_first_match_and_boundary_behavior() {
        for size in [0, 1, 31, 32, 33, 4096] {
            let mut bytes = vec![0; size];
            assert_eq!(bundle_marker(&bytes), None);
            if size >= MARKER.len() {
                let offset = size - MARKER.len();
                bytes[offset..].copy_from_slice(MARKER);
                assert_eq!(bundle_marker(&bytes), Some(offset));
                bytes[..MARKER.len()].copy_from_slice(MARKER);
                assert_eq!(bundle_marker(&bytes), Some(0));
            }
        }
    }

    #[test]
    fn component_replacement_preserves_bundle_layout() {
        for compressed in [false, true] {
            for replacement in [b"patched!".as_slice(), b"a longer replacement"] {
                let (input, entry, id) = bundle(b"original", compressed, 128);
                let output = patch_component(&input, 8, b"PCL.Core.dll", |core| {
                    assert_eq!(core, b"original");
                    Ok(Some(replacement.to_vec()))
                })
                .unwrap()
                .unwrap();
                let offset =
                    u64::from_le_bytes(output[entry..entry + 8].try_into().unwrap()) as usize;
                assert_eq!(&output[offset..offset + replacement.len()], replacement);
                assert_ne!(&output[id..id + 12], &input[id..id + 12]);
                if !compressed && replacement.len() == 8 {
                    assert_eq!(output.len(), input.len());
                    assert_eq!(offset, 64);
                } else {
                    assert_eq!(offset % 4096, 0);
                    assert!(offset >= input.len());
                    assert_eq!(&output[64..72], &input[64..72]);
                    assert_eq!(&output[entry + 16..entry + 24], &[0; 8]);
                }
                assert!(
                    patch_component(&output, 8, b"PCL.Core.dll", |core| {
                        assert_eq!(core, replacement);
                        Ok(None)
                    })
                    .unwrap()
                    .is_none()
                );
            }
        }
    }

    #[test]
    #[ignore = "performance: processes a 64 MiB bundle with a compressed 16 MiB assembly"]
    fn benchmark_large_compressed_bundle() {
        let (input, _, _) = bundle(&vec![42; 16 * 1024 * 1024], true, 64 * 1024 * 1024);
        let start = std::time::Instant::now();
        let output = patch_component(&input, 8, b"PCL.Core.dll", |core| {
            let mut output = core.to_vec();
            output[0] = 0;
            Ok(Some(output))
        })
        .unwrap()
        .unwrap();
        assert!(output.len() > input.len());
        eprintln!(
            "bundle patch: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.
        );
        std::hint::black_box(output);
    }
}
