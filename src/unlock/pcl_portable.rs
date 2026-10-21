use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    io::{Cursor, Read},
};

use dotscope::{
    CilAssemblyView, Parser,
    assembly::{Operand, decode_stream},
    metadata::{method::MethodBody, tables::*},
};
use serde::{Deserialize, Serialize};

use super::{Result, managed};

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Fixup {
    offset: usize,
    key: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Body {
    raw: Vec<u8>,
    fixups: Vec<Fixup>,
}

#[derive(Serialize, Deserialize)]
struct Template {
    method: String,
    restricted: Body,
    restored: Body,
    previous: Body,
}

struct Symbols<'a> {
    view: &'a CilAssemblyView,
    cache: RefCell<HashMap<u32, String>>,
    cache_bytes: Cell<usize>,
    owners: HashMap<u32, u32>,
    nested: HashMap<u32, u32>,
}

fn row_offset(view: &CilAssemblyView, data: &[u8], offset: usize) -> Result<usize> {
    let base = (data.as_ptr() as usize)
        .checked_sub(view.file().data().as_ptr() as usize)
        .ok_or("元数据表位置无效。")?;
    let offset = base.checked_add(offset).ok_or("元数据表偏移溢出。")?;
    if offset
        .checked_add(4)
        .is_none_or(|end| end > view.file().data().len())
    {
        return Err("元数据表超出文件范围。".into());
    }
    Ok(offset)
}

impl<'a> Symbols<'a> {
    fn new(view: &'a CilAssemblyView) -> Result<Self> {
        let tables = view.tables().ok_or("PCL 元数据表缺失。")?;
        let types = tables.table::<TypeDefRaw>().ok_or("PCL 类型表缺失。")?;
        let methods = tables.table::<MethodDefRaw>().ok_or("PCL 方法表缺失。")?;
        let fields = tables.table::<FieldRaw>().ok_or("PCL 字段表缺失。")?;
        let mut owners = HashMap::new();
        for ty in types.iter() {
            let ty = ty?;
            let next = types.get(ty.rid + 1)?;
            for rid in ty.method_list
                ..next
                    .as_ref()
                    .map_or(methods.row_count + 1, |t| t.method_list)
            {
                owners.insert(0x06000000 | rid, 0x02000000 | ty.rid);
            }
            for rid in ty.field_list..next.as_ref().map_or(fields.row_count + 1, |t| t.field_list) {
                owners.insert(0x04000000 | rid, 0x02000000 | ty.rid);
            }
        }
        let mut nested = HashMap::new();
        if let Some(rows) = tables.table::<NestedClassRaw>() {
            for row in rows.iter() {
                let row = row?;
                nested.insert(row.nested_class, 0x02000000 | row.enclosing_class);
            }
        }
        Ok(Self {
            view,
            cache: RefCell::new(HashMap::new()),
            cache_bytes: Cell::new(0),
            owners,
            nested,
        })
    }

    fn key(&self, token: u32) -> Result<String> {
        self.key_at(token, 0)
    }

    fn key_at(&self, token: u32, depth: usize) -> Result<String> {
        if depth > 64 {
            return Err("PCL 元数据引用过深或存在循环。".into());
        }
        if let Some(key) = self.cache.borrow().get(&token) {
            return Ok(key.clone());
        }
        let tables = self.view.tables().ok_or("元数据表缺失。")?;
        let strings = self.view.strings().ok_or("字符串表缺失。")?;
        let rid = token & 0xffffff;
        macro_rules! row {
            ($t:ty) => {
                tables
                    .table::<$t>()
                    .ok_or("引用表缺失。")?
                    .get(rid)?
                    .ok_or("元数据引用无效。")?
            };
        }
        let key = match token >> 24 {
            0 => "self".to_owned(),
            0x01 => {
                let r = row!(TypeRefRaw);
                format!(
                    "R{:?}",
                    (
                        self.key_at(r.resolution_scope.token.value(), depth + 1)?,
                        strings.get(r.type_namespace as usize)?,
                        strings.get(r.type_name as usize)?
                    )
                )
            }
            0x02 => {
                let r = row!(TypeDefRaw);
                let parent = self
                    .nested
                    .get(&rid)
                    .map(|t| self.key_at(*t, depth + 1))
                    .transpose()?;
                format!(
                    "D{:?}",
                    (
                        parent,
                        strings.get(r.type_namespace as usize)?,
                        strings.get(r.type_name as usize)?
                    )
                )
            }
            0x04 => {
                let r = row!(FieldRaw);
                format!(
                    "F{:?}",
                    (
                        self.key_at(
                            *self.owners.get(&token).ok_or("字段所属类型缺失。")?,
                            depth + 1
                        )?,
                        strings.get(r.name as usize)?,
                        self.signature(r.signature, false, depth + 1)?
                    )
                )
            }
            0x06 => {
                let r = row!(MethodDefRaw);
                format!(
                    "M{:?}",
                    (
                        self.key_at(
                            *self.owners.get(&token).ok_or("方法所属类型缺失。")?,
                            depth + 1
                        )?,
                        strings.get(r.name as usize)?,
                        self.signature(r.signature, false, depth + 1)?
                    )
                )
            }
            0x0a => {
                let r = row!(MemberRefRaw);
                format!(
                    "C{:?}",
                    (
                        self.key_at(r.class.token.value(), depth + 1)?,
                        strings.get(r.name as usize)?,
                        self.signature(r.signature, false, depth + 1)?
                    )
                )
            }
            0x11 => {
                let r = row!(StandAloneSigRaw);
                format!("L{}", self.signature(r.signature, false, depth + 1)?)
            }
            0x1a => {
                let r = row!(ModuleRefRaw);
                format!("N{:?}", strings.get(r.name as usize)?)
            }
            0x1b => {
                let r = row!(TypeSpecRaw);
                format!("T{}", self.signature(r.signature, true, depth + 1)?)
            }
            0x23 => {
                let r = row!(AssemblyRefRaw);
                format!(
                    "A{:?}",
                    (
                        strings.get(r.name as usize)?,
                        strings.get(r.culture as usize)?,
                        self.view
                            .blobs()
                            .ok_or("Blob 缺失。")?
                            .get(r.public_key_or_token as usize)?
                    )
                )
            }
            0x2b => {
                let r = row!(MethodSpecRaw);
                format!(
                    "G{:?}",
                    (
                        self.key_at(r.method.token.value(), depth + 1)?,
                        self.signature(r.instantiation, false, depth + 1)?
                    )
                )
            }
            0x70 => format!(
                "U{}",
                self.view
                    .userstrings()
                    .ok_or("用户字符串表缺失。")?
                    .get(rid as usize)?
                    .to_string_lossy()
            ),
            _ => return Err(format!("不支持的 PCL 元数据引用 {token:08x}。").into()),
        };
        let cache_bytes = self
            .cache_bytes
            .get()
            .checked_add(key.len())
            .ok_or("PCL 引用大小溢出。")?;
        if key.len() > 65536 || cache_bytes > 64 * 1024 * 1024 {
            return Err("PCL 引用结构超出处理范围。".into());
        }
        self.cache_bytes.set(cache_bytes);
        self.cache.borrow_mut().insert(token, key.clone());
        Ok(key)
    }

    fn signature(&self, index: u32, is_type: bool, depth: usize) -> Result<String> {
        let bytes = self
            .view
            .blobs()
            .ok_or("Blob 缺失。")?
            .get(index as usize)?;
        let mut reader = Signature {
            bytes,
            position: 0,
            symbols: self,
            depth,
        };
        let key = if is_type { reader.ty()? } else { reader.sig()? };
        if reader.position != bytes.len() {
            return Err("PCL 签名含未识别的数据。".into());
        }
        Ok(key)
    }

    fn index(&self) -> Result<HashMap<String, u32>> {
        let mut index = HashMap::new();
        macro_rules! add {
            ($t:ty, $id:expr) => {
                if let Some(rows) = self.view.tables().ok_or("元数据表缺失。")?.table::<$t>()
                {
                    for r in rows.iter() {
                        let token = ($id << 24) | r?.rid;
                        index.entry(self.key(token)?).or_insert(token);
                    }
                }
            };
        }
        add!(TypeDefRaw, 2);
        add!(TypeRefRaw, 1);
        add!(FieldRaw, 4);
        add!(MethodDefRaw, 6);
        add!(MemberRefRaw, 10);
        add!(StandAloneSigRaw, 17);
        add!(TypeSpecRaw, 27);
        add!(MethodSpecRaw, 43);
        if let Some(strings) = self.view.userstrings() {
            for (offset, value) in strings.iter() {
                index
                    .entry(format!("U{}", value.to_string_lossy()))
                    .or_insert(0x70000000 | offset as u32);
            }
        }
        Ok(index)
    }
}

// ECMA-335 signatures contain compressed TypeDefOrRef indices. Compare their
// identities rather than their numeric indices, which move between builds.
struct Signature<'a, 'b> {
    bytes: &'a [u8],
    position: usize,
    symbols: &'a Symbols<'b>,
    depth: usize,
}
impl Signature<'_, '_> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self.bytes.get(self.position).ok_or("PCL 签名不完整。")?;
        self.position += 1;
        Ok(b)
    }
    fn integer(&mut self) -> Result<u32> {
        let a = self.byte()?;
        Ok(if a & 0x80 == 0 {
            u32::from(a)
        } else if a & 0xc0 == 0x80 {
            u32::from(a & 0x3f) << 8 | u32::from(self.byte()?)
        } else if a & 0xe0 == 0xc0 {
            u32::from(a & 0x1f) << 24
                | u32::from(self.byte()?) << 16
                | u32::from(self.byte()?) << 8
                | u32::from(self.byte()?)
        } else {
            return Err("PCL 签名整数无效。".into());
        })
    }
    fn reference(&mut self) -> Result<String> {
        let n = self.integer()?;
        let table = *[2, 1, 27]
            .get((n & 3) as usize)
            .ok_or("类型签名索引无效。")?;
        self.symbols.key_at(table << 24 | n >> 2, self.depth + 1)
    }
    fn types(&mut self, count: u32) -> Result<Vec<String>> {
        if count > 65535 {
            return Err("PCL 签名参数过多。".into());
        }
        let mut types = Vec::new();
        let mut bytes = 0;
        for _ in 0..count {
            let ty = self.ty()?;
            bytes += ty.len();
            if bytes > 65536 {
                return Err("PCL 签名结构过大。".into());
            }
            types.push(ty);
        }
        Ok(types)
    }
    fn sig(&mut self) -> Result<String> {
        let flags = self.byte()?;
        if flags == 6 {
            return Ok(format!("field:{}", self.ty()?));
        }
        if matches!(flags, 7 | 10) {
            let count = self.integer()?;
            return Ok(format!("{flags}:{:?}", self.types(count)?));
        }
        if !matches!(flags & 15, 0..=5 | 8 | 9 | 11) {
            return Err("不支持的 PCL 调用约定。".into());
        }
        let generic = if flags & 16 != 0 { self.integer()? } else { 0 };
        let count = self.integer()?;
        let result = self.ty()?;
        Ok(format!(
            "{flags}:{generic}:{result}:{:?}",
            self.types(count)?
        ))
    }
    fn ty(&mut self) -> Result<String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("PCL 类型签名过深。".into());
        }
        let code = self.byte()?;
        let value = match code {
            1..=14 | 22 | 24 | 25 | 28 => format!("{code}"),
            15 | 16 | 29 | 65 | 69 => format!("{code}:{}", self.ty()?),
            17 | 18 => format!("{code}:{}", self.reference()?),
            19 | 30 => format!("{code}:{}", self.integer()?),
            20 => {
                let ty = self.ty()?;
                let rank = self.integer()?;
                let mut dimensions = Vec::new();
                for _ in 0..2 {
                    let count = self.integer()?;
                    if count > 65535 {
                        return Err("PCL 数组维度无效。".into());
                    }
                    dimensions.push(
                        (0..count)
                            .map(|_| self.integer())
                            .collect::<Result<Vec<_>>>()?,
                    );
                }
                format!("array:{ty}:{rank}:{dimensions:?}")
            }
            21 => {
                let ty = self.ty()?;
                let count = self.integer()?;
                format!("generic:{ty}:{:?}", self.types(count)?)
            }
            27 => format!("fn:{}", self.sig()?),
            31 | 32 => {
                let modifier = self.reference()?;
                format!("{code}:{modifier}:{}", self.ty()?)
            }
            _ => return Err(format!("不支持的 PCL 类型签名 {code:02x}。").into()),
        };
        self.depth -= 1;
        if value.len() > 65536 {
            return Err("PCL 类型签名过大。".into());
        }
        Ok(value)
    }
}

fn body(symbols: &Symbols<'_>, input: &[u8], method: &MethodDefRaw) -> Result<Body> {
    let offset = symbols.view.file().rva_to_offset(method.rva as usize)?;
    let bytes = input.get(offset..).ok_or("PCL 方法偏移无效。")?;
    let parsed = MethodBody::from(bytes)?;
    let code_end = parsed.size_header + parsed.size_code;
    let code = bytes
        .get(parsed.size_header..code_end)
        .ok_or("PCL 方法不完整。")?;
    let mut tokens = Vec::new();
    for instruction in decode_stream(&mut Parser::new(code), 0)? {
        if let Operand::Token(token) = instruction.operand {
            tokens.push((
                parsed.size_header + (instruction.offset + instruction.size) as usize - 4,
                token.value(),
            ));
        }
    }
    if parsed.local_var_sig_token != 0 {
        tokens.push((8, parsed.local_var_sig_token));
    }
    let mut end = code_end;
    if parsed.is_exception_data {
        end = (end + 3) & !3;
        loop {
            let header = bytes.get(end..end + 4).ok_or("PCL 异常表不完整。")?;
            if header[0] & 0x3f != 1 {
                return Err("不支持的 PCL 方法附加数据。".into());
            }
            let fat = header[0] & 0x40 != 0;
            let size = if fat {
                usize::from(header[1]) | usize::from(header[2]) << 8 | usize::from(header[3]) << 16
            } else {
                usize::from(header[1])
            };
            let stride = if fat { 24 } else { 12 };
            if size < 4 || (size - 4) % stride != 0 {
                return Err("PCL 异常表长度无效。".into());
            }
            for at in (end + 4..end + size).step_by(stride) {
                let flags = u16::from_le_bytes(
                    bytes
                        .get(at..at + 2)
                        .ok_or("PCL 异常表不完整。")?
                        .try_into()?,
                );
                if flags & 7 == 0 {
                    let at = at + if fat { 20 } else { 8 };
                    let token = u32::from_le_bytes(
                        bytes
                            .get(at..at + 4)
                            .ok_or("PCL 异常类型缺失。")?
                            .try_into()?,
                    );
                    tokens.push((at, token));
                }
            }
            end += size;
            if header[0] & 0x80 == 0 {
                break;
            }
            end = (end + 3) & !3;
        }
    }
    let mut raw = bytes.get(..end).ok_or("PCL 方法体不完整。")?.to_vec();
    let mut fixups = Vec::new();
    for (offset, token) in tokens {
        raw.get_mut(offset..offset + 4)
            .ok_or("PCL 令牌偏移无效。")?
            .fill(0);
        fixups.push(Fixup {
            offset,
            key: symbols.key(token)?,
        });
    }
    Ok(Body { raw, fixups })
}

pub(super) fn patch(input: &[u8]) -> Result<Option<Vec<u8>>> {
    patch_checked(input, true)
}

fn patch_checked(input: &[u8], verify: bool) -> Result<Option<Vec<u8>>> {
    if super::pcl::is_unlocked(input)? {
        return Ok(None);
    }
    let view = CilAssemblyView::from_reader(Cursor::new(input))?;
    if view.cor20header().managed_native_header_size != 0 {
        return Err("暂不支持含本机代码的 PCL。".into());
    }
    let symbols = Symbols::new(&view)?;
    let mut index = symbols.index()?;
    let mut json = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("patches/pcl-portable.json.gz")[..])
        .take(2 * 1024 * 1024)
        .read_to_end(&mut json)?;
    let templates: Vec<Template> = serde_json::from_slice(&json)?;
    if templates.len() != 5 {
        return Err("PCL 通用补丁数据损坏。".into());
    }
    let methods = view
        .tables()
        .ok_or("方法表缺失。")?
        .table::<MethodDefRaw>()
        .ok_or("方法表缺失。")?;
    let mut changes = Vec::new();
    for template in templates {
        let token = *index
            .get(&template.method)
            .ok_or("此 PCL 的下载方法签名已改变，暂不兼容。")?;
        let method = methods.get(token & 0xffffff)?.ok_or("PCL 方法缺失。")?;
        let current = body(&symbols, input, &method)?;
        if current == template.restored || current == template.previous {
            continue;
        }
        if current != template.restricted {
            return Err(format!(
                "PCL 下载方法 {} 的结构已改变，无法安全恢复；原文件未修改。",
                view.strings()
                    .ok_or("字符串表缺失。")?
                    .get(method.name as usize)?
            )
            .into());
        }
        changes.push((method, template.restored));
    }
    if changes.is_empty() {
        return Ok(None);
    }
    let mut strings = Vec::new();
    for (_, body) in &changes {
        for fixup in &body.fixups {
            if !index.contains_key(&fixup.key) {
                if let Some(string) = fixup.key.strip_prefix('U') {
                    if !strings.iter().any(|s| s == string) {
                        strings.push(string.to_owned());
                    }
                } else {
                    return Err("此 PCL 缺少恢复下载功能所需的成员或局部变量签名。".into());
                }
            }
        }
    }
    // Predict the appended user-string tokens; append metadata after updating MethodDef RVAs.
    let string_refs: Vec<&str> = strings.iter().map(String::as_str).collect();
    let (_, tokens) = managed::metadata_with_strings(&view, input, &string_refs)?;
    for (string, token) in strings.iter().zip(tokens) {
        index.insert(format!("U{string}"), token);
    }
    let mut output = input.to_vec();
    for (method, mut body) in changes {
        for fixup in body.fixups {
            let token = index.get(&fixup.key).ok_or("PCL 恢复引用缺失。")?;
            body.raw[fixup.offset..fixup.offset + 4].copy_from_slice(&token.to_le_bytes());
        }
        let rva = managed::append_data(&mut output, &body.raw)?;
        let offset = row_offset(&view, methods.data, method.offset)?;
        let entry = output
            .get_mut(offset..offset + 4)
            .ok_or("PCL 方法表偏移无效。")?;
        if entry != method.rva.to_le_bytes() {
            return Err("PCL 方法表位置校验失败。".into());
        }
        entry.copy_from_slice(&rva.to_le_bytes());
    }
    if !strings.is_empty() {
        let (metadata, _) = managed::metadata_with_strings(&view, &output, &string_refs)?;
        managed::relocate_metadata(&view, &mut output, &metadata)?;
    }
    if verify && patch_checked(&output, false)?.is_some() {
        return Err("PCL 通用补丁校验失败。".into());
    }
    Ok(Some(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    #[ignore = "maintainer: set UNBLOCKMC_PROFILE_DONOR, UNBLOCKMC_PROFILE_RESTRICTED and UNBLOCKMC_PROFILE_OUTPUT"]
    fn regenerate_portable_profile() {
        use std::{fs, io::Write};
        let donor = fs::read(std::env::var_os("UNBLOCKMC_PROFILE_DONOR").unwrap()).unwrap();
        use sha2::{Digest, Sha256};
        assert_eq!(
            format!("{:x}", Sha256::digest(&donor)),
            "5582952ded3b443503f66c23b810802bd20f1a3e847b54d9e09653a825859a36"
        );
        let restricted =
            fs::read(std::env::var_os("UNBLOCKMC_PROFILE_RESTRICTED").unwrap()).unwrap();
        let restored = super::super::pcl::patch(&restricted).unwrap().unwrap();
        let views = [&donor, &restricted, &restored]
            .map(|b| CilAssemblyView::from_reader(Cursor::new(b)).unwrap());
        let symbols = views.each_ref().map(|v| Symbols::new(v).unwrap());
        let mut templates = Vec::new();
        for (ty, name) in [
            ("ModModpack", "ModpackInstall"),
            ("PageDownloadInstall", "LoaderInit"),
            ("PageDownloadModify", "LoadInstanceFinished"),
            (
                "PageDownloadResourceDetail",
                "InitializeStackResourceVersions",
            ),
        ] {
            for method in managed::methods(&views[1], "PCL", ty, name).unwrap() {
                let key = symbols[1].key(method.token.value()).unwrap();
                let mut bodies = Vec::new();
                for ((view, symbols), bytes) in
                    views
                        .iter()
                        .zip(&symbols)
                        .zip([&donor, &restricted, &restored])
                {
                    let candidate = managed::methods(view, "PCL", ty, name)
                        .unwrap()
                        .into_iter()
                        .find(|m| symbols.key(m.token.value()).unwrap() == key)
                        .unwrap();
                    bodies.push(body(symbols, bytes, &candidate).unwrap());
                }
                let mut bodies = bodies.into_iter();
                templates.push(Template {
                    method: key,
                    previous: bodies.next().unwrap(),
                    restricted: bodies.next().unwrap(),
                    restored: bodies.next().unwrap(),
                });
            }
        }
        assert_eq!(templates.len(), 5);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder
            .write_all(&serde_json::to_vec(&templates).unwrap())
            .unwrap();
        fs::write(
            std::env::var_os("UNBLOCKMC_PROFILE_OUTPUT").unwrap(),
            encoder.finish().unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn embedded_templates_have_five_distinct_methods_and_valid_fixups() {
        let templates: Vec<Template> = serde_json::from_reader(flate2::read::GzDecoder::new(
            &include_bytes!("patches/pcl-portable.json.gz")[..],
        ))
        .unwrap();
        assert_eq!(templates.len(), 5);
        let keys: std::collections::HashSet<_> = templates.iter().map(|t| &t.method).collect();
        assert_eq!(keys.len(), 5);
        for template in templates {
            assert!(template.restricted != template.restored);
            for body in [template.restricted, template.restored, template.previous] {
                MethodBody::from(&body.raw).unwrap();
                for fixup in body.fixups {
                    assert_eq!(&body.raw[fixup.offset..fixup.offset + 4], &[0, 0, 0, 0]);
                    assert!(!fixup.key.is_empty());
                }
            }
        }
    }

    #[test]
    #[ignore = "requires UNBLOCKMCDOWNLOAD_PCL_EXE and UNBLOCKMC_PROFILE_DONOR"]
    fn compatible_versions_and_reordered_member_references() {
        use std::fs;
        let donor = fs::read(std::env::var_os("UNBLOCKMC_PROFILE_DONOR").unwrap()).unwrap();
        assert!(
            patch(&donor).unwrap().is_none(),
            "the older unrestricted version must remain unchanged"
        );
        let input = fs::read(std::env::var_os("UNBLOCKMCDOWNLOAD_PCL_EXE").unwrap()).unwrap();
        let view = CilAssemblyView::from_reader(Cursor::new(&input)).unwrap();
        let symbols = Symbols::new(&view).unwrap();
        let mut deep = vec![16; 65];
        deep.push(2);
        assert!(
            Signature {
                bytes: &deep,
                position: 0,
                symbols: &symbols,
                depth: 0
            }
            .ty()
            .is_err()
        );
        let wide = vec![14; 65535];
        assert!(
            Signature {
                bytes: &wide,
                position: 0,
                symbols: &symbols,
                depth: 0
            }
            .types(65535)
            .is_err()
        );
        let tables = view.tables().unwrap();
        let refs = tables.table::<MemberRefRaw>().unwrap();
        let strings = view.strings().unwrap();
        let mut selected = Vec::new();
        for row in refs.iter() {
            let row = row.unwrap();
            let name = strings.get(row.name as usize).unwrap();
            let signature = view.blobs().unwrap().get(row.signature as usize).unwrap();
            if (name == "Concat" && signature == [0, 2, 14, 14, 14])
                || (name == "Exists" && signature == [0, 1, 2, 14])
            {
                selected.push(row);
            }
        }
        let a = &selected[0];
        let b = selected
            .iter()
            .find(|r| {
                symbols.key(r.token.value()).unwrap() != symbols.key(a.token.value()).unwrap()
            })
            .unwrap();
        let first = a.token.value();
        let second = b.token.value();
        // These non-generic methods are not attribute constructors or MethodSpec parents.
        if let Some(rows) = tables.table::<MethodSpecRaw>() {
            for row in rows.iter() {
                assert!(![first, second].contains(&row.unwrap().method.token.value()));
            }
        }
        if let Some(rows) = tables.table::<CustomAttributeRaw>() {
            for row in rows.iter() {
                assert!(![first, second].contains(&row.unwrap().constructor.token.value()));
            }
        }
        let stride = refs.row_size as usize;
        let a_offset = row_offset(&view, refs.data, a.offset).unwrap();
        let b_offset = row_offset(&view, refs.data, b.offset).unwrap();
        let mut variant = input.clone();
        variant[a_offset..a_offset + stride].copy_from_slice(&input[b_offset..b_offset + stride]);
        variant[b_offset..b_offset + stride].copy_from_slice(&input[a_offset..a_offset + stride]);
        for method in tables.table::<MethodDefRaw>().unwrap().iter() {
            let method = method.unwrap();
            if method.rva == 0 {
                continue;
            }
            let offset = view.file().rva_to_offset(method.rva as usize).unwrap();
            let parsed = MethodBody::from(&input[offset..]).unwrap();
            let start = offset + parsed.size_header;
            for instruction in
                decode_stream(&mut Parser::new(&input[start..start + parsed.size_code]), 0).unwrap()
            {
                if let Operand::Token(token) = instruction.operand {
                    let other = if token.value() == first {
                        second
                    } else if token.value() == second {
                        first
                    } else {
                        continue;
                    };
                    let at = start + (instruction.offset + instruction.size) as usize - 4;
                    variant[at..at + 4].copy_from_slice(&other.to_le_bytes());
                }
            }
        }
        let assembly = tables
            .table::<AssemblyRaw>()
            .unwrap()
            .get(1)
            .unwrap()
            .unwrap();
        let assembly_offset = row_offset(
            &view,
            tables.table::<AssemblyRaw>().unwrap().data,
            assembly.offset,
        )
        .unwrap();
        variant[assembly_offset + 4..assembly_offset + 12]
            .copy_from_slice(&[3, 0, 0, 0, 0, 0, 1, 0]);
        assert!(!super::super::pcl::recognizes(
            sha2::Sha256::digest(&variant).into()
        ));
        let patched = patch(&variant).unwrap().unwrap();
        assert!(patch(&patched).unwrap().is_none());
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("multi-version.exe");
        fs::write(&file, &variant).unwrap();
        let outcome = super::super::unlock(&file).unwrap();
        assert_eq!(outcome.launcher, super::super::Launcher::Pcl);
        assert_eq!(fs::read(outcome.backup.unwrap()).unwrap(), variant);
        assert_eq!(fs::read(file).unwrap(), patched);

        let method = managed::methods(&view, "PCL", "ModModpack", "ModpackInstall")
            .unwrap()
            .into_iter()
            .find(|m| view.blobs().unwrap().get(m.signature as usize).unwrap() == [0, 0, 1])
            .unwrap();
        let at = view.file().rva_to_offset(method.rva as usize).unwrap();
        let parsed = MethodBody::from(&variant[at..]).unwrap();
        let start = at + parsed.size_header;
        let instruction = decode_stream(
            &mut Parser::new(&variant[start..start + parsed.size_code]),
            0,
        )
        .unwrap()
        .into_iter()
        .find(|i| matches!(i.opcode, 0x16..=0x1e))
        .unwrap();
        variant[start + instruction.offset as usize] = if instruction.opcode == 0x16 {
            0x17
        } else {
            0x16
        };
        let unknown = directory.path().join("changed-logic.exe");
        fs::write(&unknown, &variant).unwrap();
        assert!(super::super::unlock(&unknown).is_err());
        assert_eq!(fs::read(&unknown).unwrap(), variant);
        assert!(!unknown.with_extension("exe.bak").exists());

        let older = directory.path().join("older.exe");
        fs::write(&older, &donor).unwrap();
        let options = super::super::UnlockOptions {
            modify_auto_update: true,
        };
        assert!(
            super::super::unlock_with_options(&older, options)
                .unwrap()
                .auto_update
        );
        assert!(
            super::super::unlock_with_options(&older, options)
                .unwrap()
                .backup
                .is_none()
        );
        assert_eq!(fs::read(older.with_extension("exe.bak")).unwrap(), donor);
    }
}
