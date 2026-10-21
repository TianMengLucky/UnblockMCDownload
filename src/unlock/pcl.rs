use sha2::{Digest, Sha256};

use super::Result;

// Fast path for the known official binary; compatible builds use pcl_portable.
const PROFILE: &[u8] = include_bytes!("patches/pcl-2.99.0.0.bin");

pub(super) fn recognizes(hash: [u8; 32]) -> bool {
    hash == PROFILE[8..40] || hash == PROFILE[40..72]
}

pub(super) fn patch_with_hash(input: &[u8], hash: [u8; 32]) -> Result<Option<Vec<u8>>> {
    if hash[..] == PROFILE[40..72] {
        return Ok(None);
    }
    if hash[..] != PROFILE[8..40] || &PROFILE[..8] != b"UBMC\x01\0\0\0" {
        return Err("PCL 文件与补丁版本不匹配。".into());
    }
    let final_len = read_u32(PROFILE, 72)?;
    let mut output = Vec::with_capacity(input.len().max(final_len));
    output.extend_from_slice(input);
    output.resize(final_len, 0);
    let mut position = 76;
    while position < PROFILE.len() {
        let offset = read_u32(PROFILE, position)?;
        let length = read_u32(PROFILE, position + 4)?;
        position += 8;
        let bytes = PROFILE
            .get(position..position + length)
            .ok_or("PCL 补丁数据不完整。")?;
        output
            .get_mut(offset..offset + length)
            .ok_or("PCL 补丁写入范围无效。")?
            .copy_from_slice(bytes);
        position += length;
    }
    if Sha256::digest(&output)[..] != PROFILE[40..72] {
        return Err("PCL 修改后校验失败，原文件未修改。".into());
    }
    Ok(Some(output))
}

#[cfg(test)]
pub(super) fn patch(input: &[u8]) -> Result<Option<Vec<u8>>> {
    patch_with_hash(input, Sha256::digest(input).into())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<usize> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or("PCL 补丁数据不完整。")?;
    Ok(u32::from_le_bytes(value.try_into()?) as usize)
}

pub(super) fn is_unlocked(input: &[u8]) -> Result<bool> {
    use dotscope::{CilAssemblyView, metadata::method::MethodBody};
    use std::io::Cursor;
    let view = CilAssemblyView::from_reader(Cursor::new(input))?;
    let expected = [
        (
            "ModModpack",
            "ModpackInstall",
            123,
            "bc57d301b184ac7cdbdb1dcbaf0da441abee155da135111ee02aef967e2c74f7",
        ),
        (
            "ModModpack",
            "ModpackInstall",
            993,
            "3e09c0eeea7f6f38785bdba799263236648e84b4cf2ff2b6a39fe9ecd16c5847",
        ),
        (
            "PageDownloadInstall",
            "LoaderInit",
            51,
            "7c1d71c564ee118e49aa4741201cafcf370ab9c75064137210d574990f79525e",
        ),
        (
            "PageDownloadModify",
            "LoadInstanceFinished",
            1156,
            "4d4b28fdcc438e9d85e5b7b68ccaf048bb10c068d09789a505a546cc55fa2d92",
        ),
        (
            "PageDownloadResourceDetail",
            "InitializeStackResourceVersions",
            691,
            "1d5748bd37b2d0b0ec2690000e3a613b1fce13048296cffbb192f2da5a49b090",
        ),
    ];
    for (ty, name, size, hash) in expected {
        let mut found = false;
        for method in super::managed::methods(&view, "PCL", ty, name)? {
            let offset = view.file().rva_to_offset(method.rva as usize)?;
            let body = MethodBody::from(&input[offset..])?;
            if body.size_code == size
                && format!(
                    "{:x}",
                    Sha256::digest(&input[offset..offset + body.size_header + size])
                ) == hash
            {
                found = true;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    Ok(true)
}
