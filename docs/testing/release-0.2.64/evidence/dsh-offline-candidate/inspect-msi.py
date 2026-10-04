"""只读MSI数据库与CAB展开核验；不调用安装、注册或自定义动作。"""
import ctypes
from ctypes import wintypes as w
from pathlib import Path
import hashlib
import json
import subprocess
import time

repo = Path.cwd()
task = repo/'tmp/2026-10-02-dsh-msi'
msi_path = repo/'dist/CoolzhuAgent-0.2.64.msi'
package = repo/'tmp/candidate-064-package'
output = task/'offline-extraction-final'
if output.exists():
    raise RuntimeError('离线解包目录已存在，拒绝覆盖')
output.mkdir()
api = ctypes.WinDLL('msi.dll')
handle = w.UINT
def bind(name, args):
    f = getattr(api, name)
    f.argtypes = args
    f.restype = w.UINT
    return f
open_db = bind('MsiOpenDatabaseW', [w.LPCWSTR, w.LPCWSTR, ctypes.POINTER(handle)])
open_view = bind('MsiDatabaseOpenViewW', [handle, w.LPCWSTR, ctypes.POINTER(handle)])
execute = bind('MsiViewExecute', [handle, handle])
fetch = bind('MsiViewFetch', [handle, ctypes.POINTER(handle)])
get_string = bind('MsiRecordGetStringW', [handle, w.UINT, w.LPWSTR, ctypes.POINTER(w.DWORD)])
read_stream = bind('MsiRecordReadStream', [handle, w.UINT, ctypes.c_void_p, ctypes.POINTER(w.DWORD)])
close = bind('MsiCloseHandle', [handle])
def check(rc, action):
    if rc != 0:
        raise RuntimeError(f'{action}: MSI错误{rc}')
def string(record, field):
    length = w.DWORD(32768)
    buffer = ctypes.create_unicode_buffer(length.value)
    check(get_string(record, field, buffer, ctypes.byref(length)), '读取字段')
    return buffer.value
database = handle()
check(open_db(str(msi_path), None, ctypes.byref(database)), '只读打开MSI')
def rows(sql, field_count):
    view = handle()
    check(open_view(database, sql, ctypes.byref(view)), '打开只读查询')
    try:
        check(execute(view, 0), '执行只读查询')
        result = []
        while True:
            record = handle()
            rc = fetch(view, ctypes.byref(record))
            if rc == 259:
                break
            check(rc, '读取查询记录')
            try:
                result.append([string(record, n) for n in range(1, field_count+1)])
            finally:
                close(record)
        return result
    finally:
        close(view)
def dump_stream(name, destination):
    if "'" in name:
        raise RuntimeError('CAB流名称无效')
    view = handle()
    check(open_view(database, f"SELECT `Data` FROM `_Streams` WHERE `Name`='{name}'", ctypes.byref(view)), '打开CAB流')
    record = handle()
    try:
        check(execute(view, 0), '查询CAB流')
        check(fetch(view, ctypes.byref(record)), '读取CAB流记录')
        with destination.open('xb') as file:
            buffer = ctypes.create_string_buffer(1024*1024)
            while True:
                size = w.DWORD(len(buffer))
                check(read_stream(record, 1, buffer, ctypes.byref(size)), '读取CAB字节')
                if size.value == 0:
                    break
                file.write(buffer.raw[:size.value])
    finally:
        if record.value:
            close(record)
        close(view)
def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as file:
        for chunk in iter(lambda: file.read(1024*1024), b''):
            h.update(chunk)
    return h.hexdigest()
started = time.time()
try:
    properties = dict(rows('SELECT `Property`, `Value` FROM `Property`', 2))
    directories = {r[0]: {'parent': r[1], 'name': r[2]} for r in rows('SELECT `Directory`, `Directory_Parent`, `DefaultDir` FROM `Directory`', 3)}
    components = {r[0]: r[1] for r in rows('SELECT `Component`, `Directory_` FROM `Component`', 2)}
    files = rows('SELECT `File`, `Component_`, `FileName`, `FileSize`, `Attributes`, `Sequence` FROM `File`', 6)
    media = rows('SELECT `DiskId`, `LastSequence`, `Cabinet` FROM `Media`', 3)
    tables = {r[0] for r in rows('SELECT `Name` FROM `_Tables`', 1)}
    custom_actions = rows('SELECT `Action`, `Type`, `Source`, `Target` FROM `CustomAction`', 4) if 'CustomAction' in tables else []
    sequence = rows('SELECT `Action`, `Condition`, `Sequence` FROM `InstallExecuteSequence`', 3)
    assert properties['ProductVersion'] == '0.2.64'
    assert properties['UpgradeCode'].lower() == '{7873714f-87ee-4dfa-8aab-2a2402b4abea}'
    assert directories['INSTALLDIR']['parent'] == 'ProgramFiles64Folder'
    def target_name(name):
        name = name.split(':', 1)[0].split('|')[-1]
        if name in ('.', ''):
            return ''
        if name in ('..',) or any(c in name for c in '/\\:'):
            raise RuntimeError('安装路径包含逃逸')
        return name
    assert target_name(directories['INSTALLDIR']['name']) == 'CoolzhuAgent'
    def relative_directory(key, visited=None):
        if key == 'INSTALLDIR':
            return Path()
        visited = set() if visited is None else visited
        if key in visited or key not in directories:
            raise RuntimeError('安装目录递归无效')
        visited.add(key)
        entry = directories[key]
        return relative_directory(entry['parent'], visited)/target_name(entry['name'])
    cabinets = []
    flat = output/'cab-files'
    flat.mkdir()
    for disk_id, last_sequence, cabinet in media:
        if not cabinet.startswith('#'):
            raise RuntimeError('候选MSI要求内嵌CAB')
        name = cabinet[1:]
        if Path(name).name != name or any(c in name for c in '/\\:'):
            raise RuntimeError('CAB名称无效')
        cab_path = output/name
        dump_stream(name, cab_path)
        cp = subprocess.run(['C:/Windows/System32/expand.exe', '-F:*', str(cab_path), str(flat)], capture_output=True)
        (task/f'expand-{disk_id}.txt').write_bytes(cp.stdout+cp.stderr)
        if cp.returncode:
            raise RuntimeError(f'CAB展开失败{cp.returncode}')
        cabinets.append({'stream':name, 'size':cab_path.stat().st_size, 'sha256':sha(cab_path), 'expand_exit':cp.returncode})
    install_root = output/'layout/CoolzhuAgent'
    file_receipts = []
    paths = set()
    for file_id, component, filename, file_size, attributes, sequence_number in files:
        relative = relative_directory(components[component])/target_name(filename)
        key = relative.as_posix()
        if key.lower() in paths:
            raise RuntimeError('安装文件路径重复')
        paths.add(key.lower())
        extracted = flat/file_id
        expected = package/relative
        assert extracted.is_file(), f'CAB未包含{file_id}'
        assert expected.is_file(), f'包根缺少{key}'
        actual_sha = sha(extracted)
        assert extracted.stat().st_size == int(file_size) == expected.stat().st_size, f'大小不符{key}'
        assert actual_sha == sha(expected), f'字节摘要不符{key}'
        destination = install_root/relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(extracted.read_bytes())
        file_receipts.append({'path':key, 'file_id':file_id, 'size':int(file_size), 'sha256':actual_sha, 'sequence':int(sequence_number), 'attributes':attributes})
    expected_paths = {p.relative_to(package).as_posix().lower() for p in package.rglob('*') if p.is_file()}
    assert expected_paths == paths, f'实际MSI与包根文件集合不符：{sorted(expected_paths^paths)[:8]}'
    assert len(list(flat.iterdir())) == len(files), 'CAB包含额外文件'
    runtime = install_root/'bin/dsh-runtime'
    cp = subprocess.run(['C:/Python314/python.exe', 'scripts/prepare-dsh-runtime.py', '--verify', str(runtime)], capture_output=True)
    (task/'extracted-runtime-verifier.txt').write_bytes(cp.stdout+cp.stderr)
    assert cp.returncode == 0, '包内固定运行时核验失败'
    runtime_result = json.loads(cp.stdout)
    dsh_files = [f for f in file_receipts if f['path'].startswith('bin/dsh-runtime/')]
    assert len(dsh_files) == runtime_result['file_count'] == 290
    assert sum(f['size'] for f in dsh_files) == runtime_result['total_bytes'] == 93908058
    assert not custom_actions, f'有自定义动作，需单独审查{custom_actions}'
    result = {
        'verified': True, 'verification_kind':'只读数据库/CAB逐文件静态核验；未执行安装',
        'version':properties['ProductVersion'], 'upgrade_code':properties['UpgradeCode'],
        'msi':str(msi_path.relative_to(repo)), 'msi_size':msi_path.stat().st_size, 'msi_sha256':sha(msi_path),
        'target_layout':'ProgramFiles64Folder/CoolzhuAgent/bin/dsh-runtime',
        'file_count':len(files), 'total_bytes':sum(f['size'] for f in file_receipts),
        'cab_files_exactly_match_staged_package':True, 'cabinets':cabinets,
        'dsh_runtime':runtime_result, 'custom_actions':custom_actions,
        'install_execute_sequence_readonly':sequence, 'directories_readonly':directories,
        'files':sorted(file_receipts,key=lambda f:f['path']), 'elapsed_seconds':round(time.time()-started,3),
        'not_executed':['MSI安装/升级/卸载', '启动正式或候选GUI', '注册或恢复输入隔离', '项目模型调用'],
    }
    (task/'offline-msi-verification.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({k:v for k,v in result.items() if k not in ('files','directories_readonly','install_execute_sequence_readonly')}, ensure_ascii=False, indent=2))
finally:
    close(database)
