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
    input.windows(MARKER.len()).position(|w| w == MARKER)
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
    let mut output = input.to_vec();
    if compressed_size == 0 && patched.len() == size {
        output[offset..end].copy_from_slice(&patched);
    } else {
        // Keep every other bundle entry at its original offset; append an uncompressed replacement.
        let start = output.len().checked_add(4095).ok_or("PCL-CE 文件过大。")? & !4095;
        if start + patched.len() > super::MAX_FILE_SIZE as usize {
            return Err("PCL-CE 修改后文件超过 256 MiB。".into());
        }
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
    let mut output = input.to_vec();
    // Tiny CIL method: ldc.i4.0; ret. Keeping its RVA preserves all call sites.
    output
        .get_mut(offset..offset + 3)
        .ok_or("PCL-CE 方法体不完整。")?
        .copy_from_slice(&[0x0a, 0x16, 0x2a]);
    Ok(Some(output))
}
