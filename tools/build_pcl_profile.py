"""Reproduce the exact PCL 2.99 profile using the official 2.14 and 2.99 EXEs.

Development only: pip install dnfile==0.18.0 dncil==1.0.2
Usage: python tools/build_pcl_profile.py <2.14.exe> <2.99.exe> <profile.bin>
The two files are validated by SHA-256; this is not a general assembly importer.
"""
import argparse
import hashlib
import struct
from pathlib import Path

import dnfile
from dncil.cil.body.reader import read_method_body_from_bytes

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('donor', type=Path)
parser.add_argument('target', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
target_path, output_path = args.target, args.output
expected = {
    args.donor: '5582952ded3b443503f66c23b810802bd20f1a3e847b54d9e09653a825859a36',
    args.target: '6b6e1260cb5a1d6a303855f1c60cab3d0bc1aebba7e40a964180188b271e7386',
}
for path, digest in expected.items():
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise SystemExit(f'Unsupported input: {path}')
old = dnfile.dnPE(str(args.donor))
new = dnfile.dnPE(str(args.target))
maps = {}
substitutions = {
    '选择整合包压缩文件': '导入整合包',
    '整合包文件': '整合包',
}

def replace_string(value):
    if value.startswith('整合包种类：'):
        return '整合包安装'
    return substitutions.get(value, value)

def rows(p, table):
    return getattr(p.net.mdtables, table).rows

def map_table(table, key):
    dest = {}
    for i, row in enumerate(rows(new, table), 1):
        dest.setdefault(key(row, False), []).append(i)
    result = {}
    for i, row in enumerate(rows(old, table), 1):
        matches = dest.get(key(row, True), [])
        if len(matches) == 1:
            result[i] = matches[0]
    maps[table] = result

# Type identities retain their table indexes in these two builds.
maps['TypeDef'] = {i: i for i in range(1, len(rows(old, 'TypeDef')) + 1)}
map_table('TypeRef', lambda r, _: (r.TypeNamespace.value, r.TypeName.value))
map_table('TypeSpec', lambda r, _: r.Signature.value)
map_table('StandAloneSig', lambda r, _: r.Signature.value)

owners = {}
for p in [old, new]:
    owners[id(p)] = {}
    for tid, t in enumerate(rows(p, 'TypeDef'), 1):
        for attr in ['MethodList', 'FieldList']:
            for ref in getattr(t, attr):
                owners[id(p)][id(ref.row)] = tid

for table in ['Field', 'MethodDef']:
    map_table(table, lambda r, src: (owners[id(old if src else new)][id(r)], r.Name.value, r.Signature.value))

def ref_key(ref, src):
    table = ref.table.name
    return table, maps[table][ref.row_index] if src else ref.row_index

map_table('MemberRef', lambda r, src: (ref_key(r.Class, src), r.Name.value, r.Signature.value))
map_table('MethodSpec', lambda r, src: (ref_key(r.Method, src), r.Instantiation.value))

def remap(token):
    table, rid = token >> 24, token & 0xffffff
    if table == 0x70:
        s = old.net.user_strings.get(rid).value
        # Iterate valid entries used by new IL, rather than scanning arbitrary heap bytes.
        return 0x70000000 | strings[replace_string(s)]
    return table << 24 | maps[old.net.mdtables.tables[table].name][rid]

strings = {}
for method in rows(new, 'MethodDef'):
    if not method.Rva:
        continue
    try:
        body = read_method_body_from_bytes(new.get_data(method.Rva))
    except Exception:
        continue
    for ins in body.instructions:
        if str(ins.opcode) == 'ldstr':
            strings[new.net.user_strings.get(ins.operand.value & 0xffffff).value] = ins.operand.value & 0xffffff

def normalize(body, src):
    result = []
    for ins in body.instructions:
        operand = ins.operand
        if hasattr(operand, 'value') and 'token' in str(type(operand)).lower():
            if (operand.value >> 24) == 0x70:
                value = (old if src else new).net.user_strings.get(operand.value & 0xffffff).value
                operand = ('str', replace_string(value) if src else value)
            else:
                operand = remap(operand.value) if src else operand.value
        else:
            operand = str(operand)
        result.append((str(ins.opcode), operand))
    return result

# The compiler renumbered forwarding lambdas after inserting a new download handler.
for i, method in enumerate(rows(old, 'MethodDef'), 1):
    if not method.Name.value.startswith('_Lambda$__R') or not method.Rva:
        continue
    body = read_method_body_from_bytes(old.get_data(method.Rva))
    try:
        expected = normalize(body, True)
    except KeyError:
        continue
    matches = []
    for j, candidate in enumerate(rows(new, 'MethodDef'), 1):
        if (owners[id(old)][id(method)] == owners[id(new)][id(candidate)]
                and method.Signature.value == candidate.Signature.value and candidate.Rva
                and candidate.Name.value.startswith('_Lambda$__R')):
            other = read_method_body_from_bytes(new.get_data(candidate.Rva))
            if normalize(other, False) == expected:
                matches.append(j)
    if matches:
        maps['MethodDef'][i] = matches[0]

data = bytearray(target_path.read_bytes())
original = bytes(data)
section = new.sections[-1]
assert section.PointerToRawData + section.SizeOfRawData == len(data)

selected = {
    ('ModModpack', 'ModpackInstall'),
    ('PageDownloadInstall', 'LoaderInit'),
    ('PageDownloadModify', 'LoadInstanceFinished'),
    ('PageDownloadResourceDetail', 'InitializeStackResourceVersions'),
}
replaced = []
for t in rows(old, 'TypeDef'):
    for ref in t.MethodList:
        if (t.TypeName.value, ref.row.Name.value) not in selected:
            continue
        method = ref.row
        dest = rows(new, 'MethodDef')[maps['MethodDef'][ref.row_index] - 1]
        body = read_method_body_from_bytes(old.get_data(method.Rva))
        # dncil includes the header and EH sections in raw_bytes.
        raw = bytearray(body.raw_bytes)
        for ins in body.instructions:
            if hasattr(ins.operand, 'value') and 'token' in str(type(ins.operand)).lower():
                pos = ins.offset + len(ins.opcode.value.to_bytes(ins.opcode.size, 'little'))
                assert struct.unpack_from('<I', raw, pos)[0] == ins.operand.value
                struct.pack_into('<I', raw, pos, remap(ins.operand.value))
        if body.local_var_sig_tok:
            struct.pack_into('<I', raw, 8, remap(body.local_var_sig_tok.value))
        # Catch-class tokens in EH clauses also refer to metadata tables.
        for handler in body.exception_handlers:
            if handler.catch_type:
                token = handler.catch_type.value
                start = (body.header_size + body.code_size + 3) & ~3
                found = [i for i in range(start, len(raw) - 3) if raw[i:i+4] == struct.pack('<I', token)]
                assert found, (method.Name, handler)
                for i in found:
                    struct.pack_into('<I', raw, i, remap(token))
        data.extend(b'\0' * (-len(data) % 4))
        rva = section.VirtualAddress + len(data) - section.PointerToRawData
        struct.pack_into('<I', data, dest.struct.get_file_offset(), rva)
        data.extend(raw)
        replaced.append((t.TypeName.value, method.Name.value, rva, normalize(body, True)))

assert len(replaced) == 5
virtual_size = len(data) - section.PointerToRawData
data.extend(b'\0' * (-len(data) % new.OPTIONAL_HEADER.FileAlignment))
raw_size = len(data) - section.PointerToRawData
struct.pack_into('<I', data, section.get_file_offset() + 8, virtual_size)
struct.pack_into('<I', data, section.get_file_offset() + 16, raw_size)
# Keep the relocation directory unchanged, but retain the section and allow CIL code reads.
struct.pack_into('<I', data, section.get_file_offset() + 36, 0x60000060)
alignment = new.OPTIONAL_HEADER.SectionAlignment
image_size = (section.VirtualAddress + virtual_size + alignment - 1) & ~(alignment - 1)
struct.pack_into('<I', data, new.OPTIONAL_HEADER.get_field_absolute_offset('SizeOfImage'), image_size)
struct.pack_into('<I', data, new.OPTIONAL_HEADER.get_field_absolute_offset('CheckSum'), 0)

# Reparse every transplanted method, checking remapped IL identities and EH structure.
verified = dnfile.dnPE(data=bytes(data))
for name, method_name, rva, expected in replaced:
    body = read_method_body_from_bytes(verified.get_data(rva))
    assert normalize(body, False) == expected, (name, method_name)
    print('Restored', name, method_name, hex(rva))

source_hash = hashlib.sha256(original).digest()
output_hash = hashlib.sha256(data).digest()
# Format: magic, source hash, output hash, final length, then (offset, length, bytes).
patch = bytearray(b'UBMC\x01\0\0\0' + source_hash + output_hash + struct.pack('<I', len(data)))
offset = 0
while offset < len(data):
    if offset < len(original) and original[offset] == data[offset]:
        offset += 1
        continue
    start = offset
    while offset < len(data) and (offset >= len(original) or original[offset] != data[offset]):
        offset += 1
    patch.extend(struct.pack('<II', start, offset-start))
    patch.extend(data[start:offset])
output_path.parent.mkdir(parents=True, exist_ok=True)
output_path.write_bytes(patch)
print('source', source_hash.hex(), 'output', output_hash.hex(), 'patch bytes', len(patch))
