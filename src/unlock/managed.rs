use std::io::Cursor;

use dotscope::{
    CilAssemblyView, Parser,
    assembly::{Operand, decode_stream},
    metadata::{
        method::MethodBody,
        tables::{MemberRefRaw, MethodDefRaw, TypeDefRaw, TypeRefRaw},
    },
};

use super::Result;

pub(super) fn methods(
    view: &CilAssemblyView,
    namespace: &str,
    type_name: &str,
    method_name: &str,
) -> Result<Vec<MethodDefRaw>> {
    let tables = view.tables().ok_or(".NET 元数据表缺失。")?;
    let types = tables.table::<TypeDefRaw>().ok_or(".NET 类型表缺失。")?;
    let methods = tables.table::<MethodDefRaw>().ok_or(".NET 方法表缺失。")?;
    let strings = view.strings().ok_or(".NET 字符串表缺失。")?;
    let mut result = Vec::new();
    for row in types.iter() {
        let row = row?;
        if strings.get(row.type_namespace as usize)? != namespace
            || strings.get(row.type_name as usize)? != type_name
        {
            continue;
        }
        let end = types
            .get(row.rid + 1)?
            .map_or(methods.row_count + 1, |r| r.method_list);
        for rid in row.method_list..end {
            let method = methods.get(rid)?.ok_or(".NET 方法索引无效。")?;
            if strings.get(method.name as usize)? == method_name {
                result.push(method);
            }
        }
    }
    Ok(result)
}

fn member(view: &CilAssemblyView, token: u32) -> Result<(String, String)> {
    let tables = view.tables().ok_or(".NET 元数据表缺失。")?;
    let strings = view.strings().ok_or(".NET 字符串表缺失。")?;
    let rid = token & 0xffffff;
    match token >> 24 {
        6 => {
            let row = tables
                .table::<MethodDefRaw>()
                .ok_or("方法表缺失。")?
                .get(rid)?
                .ok_or("方法索引无效。")?;
            Ok((String::new(), strings.get(row.name as usize)?.into()))
        }
        10 => {
            let row = tables
                .table::<MemberRefRaw>()
                .ok_or("成员表缺失。")?
                .get(rid)?
                .ok_or("成员索引无效。")?;
            let class = if row.class.token.value() >> 24 == 1 {
                let ty = tables
                    .table::<TypeRefRaw>()
                    .ok_or("类型引用表缺失。")?
                    .get(row.class.row)?
                    .ok_or("类型引用无效。")?;
                strings.get(ty.type_name as usize)?.to_owned()
            } else {
                String::new()
            };
            Ok((class, strings.get(row.name as usize)?.into()))
        }
        _ => Err("自动更新方法引用不受支持。".into()),
    }
}

/// Runs an embedded bootstrap which recovers the original arguments without evaluating them.
pub(super) fn hook_update(
    input: &[u8],
    type_name: &str,
    helper: &str,
    directory: &str,
) -> Result<Option<Vec<u8>>> {
    let view = CilAssemblyView::from_reader(Cursor::new(input))?;
    if view.cor20header().managed_native_header_size != 0 {
        return Err("自动更新程序集含 ReadyToRun 本机代码，暂不支持修改。".into());
    }
    let candidates = methods(&view, "PCL", type_name, "UpdateRestart")?;
    if candidates.len() != 1 {
        return Err("未找到可修改的启动器自动更新方法。".into());
    }
    let method = &candidates[0];
    let offset = view.file().rva_to_offset(method.rva as usize)?;
    let body = MethodBody::from(input.get(offset..).ok_or("自动更新方法偏移无效。")?)?;
    let start = offset + body.size_header;
    let code = input
        .get(start..start + body.size_code)
        .ok_or("自动更新方法体不完整。")?;
    let instructions = decode_stream(&mut Parser::new(code), 0)?;
    let prefix = super::bootstrap::command_prefix(helper, directory);
    let strings = view.userstrings().ok_or(".NET 用户字符串表缺失。")?;
    for instruction in &instructions {
        if instruction.opcode == 0x72
            && let Operand::Token(token) = instruction.operand
        {
            let value = strings
                .get((token.value() & 0xffffff) as usize)?
                .to_string_lossy();
            if value == prefix {
                return Ok(None);
            }
        }
    }
    let constructor = instructions
        .iter()
        .position(|instruction| {
            instruction.opcode == 0x73
                && match instruction.operand {
                    Operand::Token(token) => member(&view, token.value())
                        .is_ok_and(|(ty, name)| ty == "ProcessStartInfo" && name == ".ctor"),
                    _ => false,
                }
        })
        .ok_or("自动更新流程未使用预期的 ProcessStartInfo。")?;
    let load = constructor
        .checked_sub(1)
        .and_then(|i| instructions.get(i))
        .ok_or("自动更新文件名参数缺失。")?;
    let local_store: Vec<u8> = match load.opcode {
        0x06..=0x09 => vec![load.opcode + 4],
        0x11 => vec![0x13, code[load.offset as usize + 1]],
        _ => return Err("自动更新文件名参数结构不匹配。".into()),
    };
    let prior_store = instructions[..constructor - 1]
        .iter()
        .rposition(|i| matches!(i.opcode, 0x0a..=0x0d | 0x13))
        .ok_or("自动更新命令行参数结构不匹配。")?;
    let begin = (instructions[prior_store].offset + instructions[prior_store].size) as usize;
    let end = load.offset as usize;
    let store = &instructions[prior_store];
    let args_store = code[store.offset as usize..(store.offset + store.size) as usize].to_vec();
    let mut args_load = args_store.clone();
    args_load[0] = if args_load[0] == 0x13 {
        0x11
    } else {
        args_load[0] - 4
    };
    if end < begin + 15 + args_store.len() + args_load.len() + local_store.len() {
        return Err("自动更新插入位置空间不足。".into());
    }
    let logging = &instructions[prior_store + 1..constructor - 1];
    let mut saw_log = false;
    for instruction in logging {
        if instruction.opcode == 0x28 {
            let Operand::Token(token) = instruction.operand else {
                return Err("自动更新日志引用无效。".into());
            };
            let (_, name) = member(&view, token.value())?;
            if matches!(name.as_str(), "Log" | "Info") {
                saw_log = true;
            } else if !matches!(name.as_str(), "Concat" | "Format") {
                return Err("自动更新插入位置包含其他操作。".into());
            }
        } else if !matches!(instruction.opcode, 0x00 | 0x06..=0x09 | 0x11 | 0x14..=0x20 | 0x72) {
            return Err("自动更新日志代码结构不匹配。".into());
        }
    }
    if !saw_log
        || instructions.iter().any(|i| {
            i.branch_targets
                .iter()
                .any(|&target| (begin as u64..end as u64).contains(&target))
        })
    {
        return Err("自动更新插入位置存在分支或缺少日志调用。".into());
    }
    let concat = view
        .tables()
        .ok_or("元数据表缺失。")?
        .table::<MemberRefRaw>()
        .ok_or("成员引用表缺失。")?
        .iter()
        .find_map(|row| {
            let row = row.ok()?;
            let token = 0x0a000000 | row.rid;
            let (class, name) = member(&view, token).ok()?;
            (class == "String"
                && name == "Concat"
                && view.blobs()?.get(row.signature as usize).ok()?
                    == [0x00, 0x02, 0x0e, 0x0e, 0x0e])
            .then_some(token)
        })
        .ok_or("未找到双字符串拼接引用。")?;
    let powershell =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").ok_or("SystemRoot 未设置。")?)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
    let powershell = powershell.to_str().ok_or("PowerShell 路径无效。")?;
    let (metadata, tokens) = metadata_with_strings(&view, input, &[&prefix, powershell])?;
    let mut injection = vec![0x72];
    injection.extend_from_slice(&tokens[0].to_le_bytes());
    injection.extend_from_slice(&args_load);
    injection.push(0x28);
    injection.extend_from_slice(&concat.to_le_bytes());
    injection.extend_from_slice(&args_store);
    injection.push(0x72);
    injection.extend_from_slice(&tokens[1].to_le_bytes());
    injection.extend_from_slice(&local_store);
    let mut output = input.to_vec();
    let region = &mut output[start + begin..start + end];
    region.fill(0);
    region[..injection.len()].copy_from_slice(&injection);
    relocate_metadata(&view, &mut output, &metadata)?;
    drop(view);
    let reloaded = CilAssemblyView::from_reader(Cursor::new(&output))?;
    if !reloaded
        .userstrings()
        .ok_or("更新补丁校验失败。")?
        .contains(&prefix)
    {
        return Err("自动更新补丁校验失败。".into());
    }
    Ok(Some(output))
}

pub(super) fn metadata_with_strings(
    view: &CilAssemblyView,
    input: &[u8],
    values: &[&str],
) -> Result<(Vec<u8>, Vec<u32>)> {
    let root_offset = view
        .file()
        .rva_to_offset(view.cor20header().meta_data_rva as usize)?;
    let root = input
        .get(root_offset..root_offset + view.cor20header().meta_data_size as usize)
        .ok_or("元数据范围无效。")?;
    if !root.starts_with(b"BSJB") {
        return Err("元数据签名无效。".into());
    }
    let mut position = 16 + read_u32(root, 12)? as usize;
    let count = read_u16(root, position + 2)?;
    position += 4;
    let mut streams = Vec::new();
    for _ in 0..count {
        let header = position;
        let offset = read_u32(root, position)? as usize;
        let size = read_u32(root, position + 4)? as usize;
        position += 8;
        let name_len = root
            .get(position..)
            .ok_or("元数据流名缺失。")?
            .iter()
            .position(|&b| b == 0)
            .ok_or("元数据流名无效。")?;
        let name = &root[position..position + name_len];
        position = (position + name_len + 4) & !3;
        streams.push((header, offset, size, name == b"#US"));
    }
    let mut output = root.get(..position).ok_or("元数据头无效。")?.to_vec();
    let mut tokens = Vec::new();
    for (header, offset, size, user_strings) in streams {
        output.resize((output.len() + 3) & !3, 0);
        let start = output.len();
        output.extend_from_slice(
            root.get(offset..offset + size)
                .ok_or("元数据流范围无效。")?,
        );
        if user_strings {
            for value in values {
                let offset = output.len() - start;
                if offset > 0xffffff {
                    return Err("用户字符串表过大。".into());
                }
                tokens.push(0x70000000 | offset as u32);
                let encoded: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
                let length = u32::try_from(encoded.len() + 1)?;
                if length < 0x80 {
                    output.push(length as u8);
                } else if length < 0x4000 {
                    output.extend_from_slice(&((length as u16) | 0x8000).to_be_bytes());
                } else if length < 0x20000000 {
                    output.extend_from_slice(&(length | 0xc0000000).to_be_bytes());
                } else {
                    return Err("更新脚本过长。".into());
                }
                output.extend_from_slice(&encoded);
                output.push(1);
            }
        }
        output.resize((output.len() + 3) & !3, 0);
        write_u32(&mut output, header, u32::try_from(start)?)?;
        let length = output.len() - start;
        write_u32(&mut output, header + 4, u32::try_from(length)?)?;
    }
    if tokens.len() != values.len() {
        return Err("元数据缺少用户字符串流。".into());
    }
    Ok((output, tokens))
}

pub(super) fn relocate_metadata(
    view: &CilAssemblyView,
    output: &mut Vec<u8>,
    metadata: &[u8],
) -> Result<()> {
    let pe = read_u32(output, 0x3c)? as usize;
    let optional = pe + 24;
    let directory = optional
        + match read_u16(output, optional)? {
            0x10b => 96,
            0x20b => 112,
            _ => return Err("PE 格式不受支持。".into()),
        };
    let clr = view
        .file()
        .rva_to_offset(read_u32(output, directory + 14 * 8)? as usize)?;
    let rva = append_data(output, metadata)?;
    write_u32(output, clr + 8, rva)?;
    write_u32(output, clr + 12, u32::try_from(metadata.len())?)?;
    Ok(())
}

pub(super) fn append_data(output: &mut Vec<u8>, metadata: &[u8]) -> Result<u32> {
    let pe = read_u32(output, 0x3c)? as usize;
    if output.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("PE 头无效。".into());
    }
    let count = read_u16(output, pe + 6)? as usize;
    if count == 0 {
        return Err("PE 节表为空。".into());
    }
    let optional = pe + 24;
    let section = optional + read_u16(output, pe + 20)? as usize + (count - 1) * 40;
    let virtual_address = read_u32(output, section + 12)? as usize;
    let raw_size = read_u32(output, section + 16)? as usize;
    let raw_start = read_u32(output, section + 20)? as usize;
    if raw_start.checked_add(raw_size) != Some(output.len()) {
        return Err("暂不支持带附加数据的 .NET 程序集。".into());
    }
    let file_alignment = read_u32(output, optional + 36)? as usize;
    let section_alignment = read_u32(output, optional + 32)? as usize;
    if !file_alignment.is_power_of_two() || !section_alignment.is_power_of_two() {
        return Err("PE 对齐参数无效。".into());
    }
    output.resize((output.len() + 3) & !3, 0);
    let metadata_rva = virtual_address + output.len() - raw_start;
    output.extend_from_slice(metadata);
    let virtual_size = output.len() - raw_start;
    let final_length = (output.len() + file_alignment - 1) & !(file_alignment - 1);
    if final_length > super::MAX_FILE_SIZE as usize {
        return Err("修改后的程序集过大。".into());
    }
    output.resize(final_length, 0);
    write_u32(output, section + 8, u32::try_from(virtual_size)?)?;
    write_u32(
        output,
        section + 16,
        u32::try_from(final_length - raw_start)?,
    )?;
    write_u32(output, section + 36, 0x60000060)?;
    let image_size =
        (virtual_address + virtual_size + section_alignment - 1) & !(section_alignment - 1);
    write_u32(output, optional + 56, u32::try_from(image_size)?)?;
    write_u32(output, optional + 64, 0)?;
    Ok(u32::try_from(metadata_rva)?)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or("程序集数据不完整。")?
            .try_into()?,
    ))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("程序集数据不完整。")?
            .try_into()?,
    ))
}
fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<()> {
    bytes
        .get_mut(offset..offset + 4)
        .ok_or("程序集写入范围无效。")?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}
