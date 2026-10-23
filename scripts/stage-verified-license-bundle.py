"""将已核许可证复用于本次实际构建；组件或原生产物变化时拒绝沿用。"""
import argparse
import hashlib
import json
from pathlib import Path


def read(path):
    return json.loads(Path(path).read_text('utf-8-sig'))


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def inside(root, relative):
    path = (root / relative).resolve(strict=True)
    path.relative_to(root)
    return path


def stage(root, bundle, payload, captures, output):
    root, bundle, payload = (Path(p).resolve(strict=True) for p in (root, bundle, payload))
    output = Path(output).absolute()
    if output.exists():
        raise ValueError('许可证输出目录已存在，禁止覆盖既有证据')
    manifest = read(bundle / 'components.json')
    if not manifest.get('complete') or manifest.get('missing') or not manifest.get('non_cargo'):
        raise ValueError('许可证包未完整覆盖Cargo及非Cargo组件')
    components = {x['id']: x for x in manifest['components']}
    if len(components) != len(manifest['components']):
        raise ValueError('重复组件身份')
    texts = {}
    for component in [*components.values(), *manifest['non_cargo']['components']]:
        if not component['files']:
            raise ValueError('组件缺少许可证原文')
        for item in component['files']:
            path = inside(bundle, item['path'])
            if digest(path) != item['sha256']:
                raise ValueError('许可证原文字节漂移')
            texts[item['path']] = path.read_bytes()
            if item['source_scope'] == 'workspace-inherited':
                if digest(inside(root, item['source_path'])) != item['sha256']:
                    raise ValueError('工作区许可证改变，需重新审核')
    evidence = read(captures)
    if not isinstance(evidence, list) or not evidence:
        raise ValueError('必须提供本次实际构建回执，不能使用SkipBuild冒充')
    observed = set()
    for entry in evidence:
        path = inside(root, entry['path'])
        if digest(path) != entry['sha256']:
            raise ValueError('构建回执摘要不符')
        finished = False
        with path.open(encoding='utf-8-sig') as stream:
            while line := stream.readline(4 * 1024 * 1024 + 1):
                if len(line) > 4 * 1024 * 1024:
                    raise ValueError('构建回执单行超限')
                if not line.strip():
                    continue
                if finished:
                    raise ValueError('构建回执超限或终态后仍有消息')
                row = json.loads(line)
                if row.get('reason') == 'build-finished':
                    if row.get('success') is not True:
                        raise ValueError('构建失败')
                    finished = True
                if row.get('reason') in ('compiler-artifact', 'build-script-executed'):
                    observed.add(row['package_id'])
        if not finished:
            raise ValueError('构建缺少成功终态')
    missing = sorted(observed - components.keys())
    if not observed or missing:
        raise ValueError('本次构建出现尚未审核许可证的组件：' + repr(missing))
    # 逐一匹配当前载荷中的包清单和运行库，新增依赖不能被旧许可证全集掩盖。
    noncargo = manifest['non_cargo']['components']
    covered = {}
    for component in noncargo:
        key = component.get('manifest') or component.get('binary')
        expected = component.get('manifest_sha256') or component.get('binary_sha256')
        if key in covered:
            raise ValueError('重复非Cargo组件路径')
        if digest(inside(payload, key)) != expected:
            raise ValueError('非Cargo组件已改变：' + key)
        covered[key] = expected
        for item in component['files']:
            if item['source_scope'] == 'actual-payload' and digest(inside(payload, item['source_path'])) != item['sha256']:
                raise ValueError('载荷许可证改变：' + item['source_path'])
    actual = {p.relative_to(payload).as_posix() for p in payload.rglob('*')
              if p.is_file() and (p.name in ('package.json', 'node.exe') or p.suffix.lower() == '.dll')}
    if actual != covered.keys():
        raise ValueError('非Cargo载荷集合改变：' + repr(sorted(actual ^ covered.keys())))
    proof = {'schema': 1, 'status': 'verified', 'scope': '本次实际构建观察全集及非Cargo载荷；不宣称精确运行时链接全集',
             'source_bundle_sha256': digest(bundle / 'components.json'),
             'source_notices_sha256': digest(bundle / 'THIRD-PARTY-NOTICES.md'),
             'build_captures': evidence, 'observed_cargo_count': len(observed),
             'license_catalog_count': len(components), 'non_cargo_count': len(covered),
             'observed_cargo_ids': sorted(observed), 'payload_components': covered}
    # 通过全部校验才创建输出；不用复制整个旧包，不改变旧发布证据。
    output.mkdir(parents=True)
    (output / 'texts').mkdir()
    for name, raw in texts.items():
        target = output / name
        target.resolve().relative_to(output.resolve())
        target.write_bytes(raw)
    for name in ('components.json', 'THIRD-PARTY-NOTICES.md'):
        (output / name).write_bytes((bundle / name).read_bytes())
    (output / 'current-build-coverage.json').write_text(json.dumps(proof, ensure_ascii=False, indent=2) + '\n', 'utf-8')
    return proof


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'bundle', 'payload', 'captures', 'output'):
        parser.add_argument('--' + name, required=True)
    args = parser.parse_args()
    proof = stage(args.root, args.bundle, args.payload, args.captures, args.output)
    print(json.dumps({key: proof[key] for key in ('status', 'observed_cargo_count', 'non_cargo_count')}, ensure_ascii=False))
