#!/usr/bin/env python3
"""Collect auditable notices for locked Rust dependencies.

Crate archives sometimes omit a workspace license. Supplemental notices below
are verified against the exact crate release commit and a reviewed SHA-256.
The output records every source and any unresolved non-release dependency.
A missing release or build dependency notice stops packaging.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / 'tmp/license-cache'
NOTICE_PREFIXES = ('license', 'copying', 'unlicense', 'notice')
# Reviewed upstream sources for exact Cargo.lock versions. Never use a moving tag.
SUPPLEMENTAL = [
    ('DoumanAsh/clipboard-win', '3b27cf2bfd1adcfa6e0264eb51c1025ddaf0f342',
     'clipboard-win-5.4.1', (
         ('LICENSE', 'c9bff75738922193e67fa726fa225535870d2aa1059f91452c411736284ad566'),
     )),
    ('Spxg/sqlite-wasm-rs', '1bb309784f25c401ddf95131dcde4c4cdad7aee7',
     'rsqlite-vfs-0.1.1', (
         ('LICENSE', 'e1c1975c3474cce31722836f101a40ffaf1cc784e4d2f38ce3e77b9712934e78'),
     )),
    ('aclysma/profiling', '8271551172eb6fa4cba47369aedd93790c623df9',
     'profiling-1.0.18', (
         ('LICENSE-APACHE', '10d30a673cd5e9349bdc02aeb48f14b3386d27d0da32df8f0a555d4aa16aa551'),
         ('LICENSE-MIT', 'c8167fdeeed46d3f244d3f85c5bf998ce889343691c32be2c61a8bc4b5c08333'),
     )),
    ('etemesi254/zune-image', 'f8fbb123d5ed04441e8324a555bfcda0cb1bd28f',
     'zune-core-0.4.12', (
         ('LICENSE-ZLIB', '7fa429541e55b1509909e058f2d21a37467e4958ec713b357f6e0cf9dc4ee352'),
         ('LICENSE.md', 'c6dff146a9f31848ac296faa5a08a4253caf2c384c86f906dc99e7fc0a39cc8c'),
     )),
    ('etemesi254/zune-image', 'fa2c767a01d7d9373911d0bf63e0588553d67e0e',
     'zune-jpeg-0.4.21', (
         ('LICENSE-ZLIB', '7fa429541e55b1509909e058f2d21a37467e4958ec713b357f6e0cf9dc4ee352'),
         ('LICENSE.md', 'c6dff146a9f31848ac296faa5a08a4253caf2c384c86f906dc99e7fc0a39cc8c'),
     )),
    ('iced-rs/iced', '0ecf60664df7b8ac7d7aef5f7279d5323027f693',
     'iced_tiny_skia-0.14.1', (
         ('LICENSE', 'fc9086ba4eba4b77e4c603906362742bc3715ffa2c7810c6074fe7780d3a0398'),
     )),
    ('iced-rs/iced', '38237dd294da3256c2ac4d3f8288fc73956c91e0',
     'iced_winit-0.14.1', (
         ('LICENSE', 'fc9086ba4eba4b77e4c603906362742bc3715ffa2c7810c6074fe7780d3a0398'),
     )),
    ('iced-rs/iced', '3997291f318a8bc06fa522f5579836fb3feb94df',
     'iced_core-0.14.0 iced_debug-0.14.0 iced_futures-0.14.0 iced_graphics-0.14.0 iced_program-0.14.0 iced_renderer-0.14.0 iced_runtime-0.14.0 iced_selector-0.14.0 iced_test-0.14.0 iced_wgpu-0.14.0', (
         ('LICENSE', 'fc9086ba4eba4b77e4c603906362742bc3715ffa2c7810c6074fe7780d3a0398'),
     )),
    ('iced-rs/iced', '54abf81d13fec06d4d9ac754b03ce2c3313e8a1f',
     'iced_widget-0.14.2', (
         ('LICENSE', 'fc9086ba4eba4b77e4c603906362742bc3715ffa2c7810c6074fe7780d3a0398'),
     )),
    ('jni-rs/jni-rs', '33045a124105c939d1e2cbdcb5a39e5d868ffa03',
     'jni-macros-0.22.4', (
         ('LICENSE-APACHE', 'a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2'),
         ('LICENSE-MIT', 'fea1d5bf3dd71605ce5d7d2ff695c1837e914c77195e523a86b5391716477960'),
     )),
    ('jni-rs/jni-rs', '5ae9458a4ec44c5318f37ddc7569c1d4ae8a69e7',
     'jni-0.22.4', (
         ('LICENSE-APACHE', 'a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2'),
         ('LICENSE-MIT', 'fea1d5bf3dd71605ce5d7d2ff695c1837e914c77195e523a86b5391716477960'),
     )),
    ('jni-rs/jni-sys', '64d77b7a5f119d7b55b4e2c169a4668067ff59e6',
     'jni-sys-macros-0.4.1', (
         ('LICENSE-APACHE', 'c6596eb7be8581c18be736c846fb9173b69eccf6ef94c5135893ec56bd92ba08'),
         ('LICENSE-MIT', '1d85bd754b04ceec93e98e890edd1fa3c6a22e81bcb32135806beeccefa51cd1'),
     )),
    ('madsmtm/objc2', '4fc083f1c6d6784577e38b0ee8dbd344481e2fd2',
     'block2-0.5.1 objc-sys-0.3.5 objc2-0.5.2', (
         ('LICENSE.txt', 'e353f37b12aefbb9f9b29490e837cfee05d9bda70804b3562839a3285c1df1e5'),
     )),
    ('madsmtm/objc2', '7b1abfd750a2cacaea71d6a56ecfb83cb7de560b',
     'objc2-core-foundation-0.3.2 objc2-core-graphics-0.3.2 objc2-foundation-0.3.2 objc2-io-surface-0.3.2 objc2-quartz-core-0.3.2', (
         ('LICENSE.md', '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54'),
     )),
    ('madsmtm/objc2', '8852b424193ca41602281b3d7540d7c8ed51e49a',
     'dispatch2-0.3.1 objc2-0.6.4', (
         ('LICENSE.md', '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54'),
     )),
    ('madsmtm/objc2', '8d214f5477365ffcbcbb7de058c86ed9a518efb7',
     'objc2-encode-4.1.0', (
         ('LICENSE.md', '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54'),
     )),
    ('madsmtm/objc2', 'b4167b582b2f75f9a1be75495c41b765344fd03c',
     'block2-0.6.2', (
         ('LICENSE.md', '7f976f7e9cb2d87df7230606feb932c3f21ac0e664045a775b600046ff850c54'),
     )),
    ('madsmtm/objc2', 'e282618be4c3a3b9542957e0c8540e9588472ce8',
     'objc2-app-kit-0.2.2 objc2-cloud-kit-0.2.2 objc2-contacts-0.2.2 objc2-core-data-0.2.2 objc2-core-image-0.2.2 objc2-core-location-0.2.2 objc2-foundation-0.2.2 objc2-link-presentation-0.2.2 objc2-metal-0.2.2 objc2-quartz-core-0.2.2 objc2-symbols-0.2.2 objc2-ui-kit-0.2.2 objc2-uniform-type-identifiers-0.2.2 objc2-user-notifications-0.2.2', (
         ('LICENSE.txt', 'e353f37b12aefbb9f9b29490e837cfee05d9bda70804b3562839a3285c1df1e5'),
     )),
    ('nical/rust_debug', '93f414cd572b5c2781df93dbdc068ed13443d05a',
     'svg_fmt-0.4.5', (
         ('LICENSE', '7e98bed7f100747aaae7d82ab94fbe6b76fd6b84b09a3aaa413bff8196d0fbb1'),
     )),
    ('rust-mobile/ndk', '49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25',
     'ndk-0.9.0 ndk-sys-0.6.0+11769913', (
         ('LICENSE-APACHE', 'c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4'),
         ('LICENSE-MIT', '508a77d2e7b51d98adeed32648ad124b7b30241a8e70b2e72c99f92d8e5874d1'),
     )),
    ('rust-windowing/android-ndk-rs', '10f2ba388fca20f7349996ebae26ccda7a6fda5c',
     'ndk-context-0.1.1', (
         ('LICENSE-APACHE', 'c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4'),
         ('LICENSE-MIT', '508a77d2e7b51d98adeed32648ad124b7b30241a8e70b2e72c99f92d8e5874d1'),
     )),
]

SUPPLEMENTAL_BY_CRATE = {
    name: (repository, revision, files)
    for repository, revision, names, files in SUPPLEMENTAL
    for name in names.split()
}
# These manifests declare a license, but the release sources do not provide its
# complete text. Keep the declaration and report the gap; do not invent a grant.
DECLARATION_ONLY = {'dispatch-0.2.0', 'hexf-parse-0.2.1'} | {
    name for repository, _, names, files in SUPPLEMENTAL
    if repository == 'madsmtm/objc2' and files[0][0] == 'LICENSE.md'
    for name in names.split()
}
# winapi's target import-library crates share the repository-wide license and
# author of winapi 0.3.9. Their archives omit it and have no VCS metadata.
SHARED_NOTICES = {
    'winapi-i686-pc-windows-gnu-0.4.0': 'winapi-0.3.9',
    'winapi-x86_64-pc-windows-gnu-0.4.0': 'winapi-0.3.9',
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def repository_name(url):
    if not url:
        return None
    match = re.match(r'https?://github\.com/([^/]+/[^/]+)', url)
    return match.group(1).removesuffix('.git').lower() if match else url


def local_notices(package):
    source = Path(package['manifest_path']).parent
    paths = []
    for path in sorted(source.iterdir()):
        if path.name.lower().startswith(NOTICE_PREFIXES):
            paths.extend([path] if path.is_file() else sorted(
                entry for entry in path.rglob('*') if entry.is_file()))
    # r-efi intentionally puts its complete MIT grant and copyright in AUTHORS.
    if package['name'] == 'r-efi' and package['version'] in ('5.3.0', '6.0.0'):
        paths.append(source / 'AUTHORS')
    if package.get('license_file'):
        path = source / package['license_file']
        if path.is_file():
            paths.append(path)
    return sorted(set(paths))


def fetch_notice(url, expected, offline=False, cache=None):
    cache = CACHE if cache is None else Path(cache)
    path = cache / expected
    if path.exists():
        data = path.read_bytes()
    else:
        if offline:
            raise RuntimeError(f'License notice is not cached: {url}. Run the notice builder online once.')
        request = urllib.request.Request(url, headers={'User-Agent': 'Cartridge-Studio-license-builder'})
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read(256 * 1024 + 1)
        if len(data) > 256 * 1024:
            raise RuntimeError(f'Upstream license notice is unexpectedly large: {url}')
    if digest(data) != expected:
        raise RuntimeError(f'License checksum mismatch: {url}. Remove its cache entry and investigate upstream provenance.')
    if not path.exists():
        cache.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return data


def release_dependencies(environment, target):
    # Unlike full cargo metadata, this excludes development and other-target
    # edges. Build dependencies are deliberately included as a stricter gate.
    listing = subprocess.check_output([
        'cargo', 'tree', '--locked', '--offline', '--workspace', '--target', target,
        '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}',
    ], cwd=ROOT, env=environment, text=True)
    return {f'{match[1]}-{match[2]}' for line in listing.splitlines()
            if (match := re.match(r'^(\S+) v([^ ]+)', line))}


def collect(packages, release_ids, output, offline=False):
    packages = sorted((p for p in packages if p['source'] is not None),
                      key=lambda p: (p['name'], p['version']))
    by_name = {f'{p["name"]}-{p["version"]}': p for p in packages}
    # Content equality and upstream identity both must match before reuse.
    known = {}
    for name, package in by_name.items():
        source = Path(package['manifest_path']).parent
        for path in local_notices(package):
            known[(repository_name(package.get('repository')), digest(path.read_bytes()))] = (
                name, path, path.relative_to(source).as_posix())
    records = []
    for name, package in by_name.items():
        source = Path(package['manifest_path']).parent
        directory = output / name
        directory.mkdir(parents=True)
        record = {key: package.get(key) for key in ('name', 'version', 'license', 'repository', 'source')}
        record.update(release_dependency=name in release_ids, notice_status='complete', notices=[])
        vcs_path = source / '.cargo_vcs_info.json'
        vcs = json.loads(vcs_path.read_text()) if vcs_path.exists() else {}
        record['source_revision'] = vcs.get('git', {}).get('sha1')

        def include(relative, data, provenance):
            target = directory / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            record['notices'].append({'file': f'{name}/{relative}', 'sha256': digest(data), **provenance})

        for path in local_notices(package):
            relative = path.relative_to(source).as_posix()
            include(relative, path.read_bytes(), {'origin': 'crate-archive', 'path_in_crate': relative})
        if not record['notices'] and name in SHARED_NOTICES:
            donor_name = SHARED_NOTICES[name]
            donor = by_name.get(donor_name)
            if donor is None or repository_name(donor.get('repository')) != repository_name(package.get('repository')):
                raise RuntimeError(f'Review the shared upstream license mapping for {name}.')
            if donor.get('license') != package.get('license') or donor.get('authors') != package.get('authors'):
                raise RuntimeError(f'Upstream license or author changed for {name}; review shared notices.')
            donor_source = Path(donor['manifest_path']).parent
            for path in local_notices(donor):
                include(path.relative_to(donor_source).as_posix(), path.read_bytes(), {
                    'origin': 'same-upstream-crate', 'shared_with': donor_name,
                    'source_revision': json.loads((donor_source / '.cargo_vcs_info.json').read_text())['git']['sha1'],
                })
        if not record['notices'] and name in SUPPLEMENTAL_BY_CRATE:
            repository, revision, files = SUPPLEMENTAL_BY_CRATE[name]
            declared_repository = repository_name(package.get('repository'))
            if record['source_revision'] != revision or (declared_repository and declared_repository != repository.lower()):
                raise RuntimeError(f'Upstream source changed for {name}; review its supplemental license mapping.')
            record['resolved_repository'] = f'https://github.com/{repository}'
            for filename, expected in files:
                url = f'https://raw.githubusercontent.com/{repository}/{revision}/{filename}'
                same = known.get((repository.lower(), expected))
                if same:
                    donor_name, path, relative = same
                    data = path.read_bytes()
                    provenance = {'origin': 'same-upstream-crate', 'shared_with': donor_name,
                                  'path_in_crate': relative, 'url': url, 'source_revision': revision}
                else:
                    data = fetch_notice(url, expected, offline)
                    provenance = {'origin': 'pinned-upstream', 'url': url, 'source_revision': revision}
                include(filename, data, provenance)
        if not record['notices'] or name in DECLARATION_ONLY:
            record['notice_status'] = 'upstream-declaration-only' if name in DECLARATION_ONLY else 'missing'
            include('UPSTREAM-Cargo.toml', (source / 'Cargo.toml').read_bytes(), {
                'origin': 'crate-archive', 'path_in_crate': 'Cargo.toml',
                'note': 'License declaration only; not a replacement for missing upstream license text.',
            })
        if name in ('zune-core-0.4.12', 'zune-jpeg-0.4.21'):
            record['license_choice'] = 'Zlib'
        if package['name'] == 'r-efi':
            record['license_choice'] = 'MIT'
        records.append(record)
    blockers = [f'{r["name"]}-{r["version"]}' for r in records
                if r['release_dependency'] and r['notice_status'] != 'complete']
    if blockers:
        raise RuntimeError('Cannot package unresolved release/build license notices: ' + ', '.join(blockers))
    return records


def build(target=None, offline=False):
    environment = os.environ.copy()
    environment['CARGO_HOME'] = str(ROOT / 'tmp/toolchains/cargo')
    if target is None:
        compiler = subprocess.check_output(['rustc', '-vV'], text=True)
        target = next(line.split(': ', 1)[1] for line in compiler.splitlines() if line.startswith('host: '))
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--offline', '--format-version=1',
    ], cwd=ROOT, env=environment))
    release_ids = release_dependencies(environment, target)
    output = ROOT / 'tmp/rust-licenses'
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='rust-notices-', dir=output.parent) as temporary:
        stage = Path(temporary) / 'licenses'
        stage.mkdir()
        records = collect(metadata['packages'], release_ids, stage, offline)
        unresolved = [f'{r["name"]}-{r["version"]}' for r in records if r['notice_status'] != 'complete']
        (stage / 'rust-components.json').write_text(json.dumps({
            'scope': 'Cargo.lock workspace dependencies, including target-specific and development packages',
            'release_target': target,
            'release_scope': 'Normal and build dependencies; excludes development and other-target edges',
            'unresolved_nonrelease_notices': unresolved,
            'components': records,
        }, indent=2) + '\n')
        if output.exists():
            shutil.rmtree(output)
        stage.replace(output)
    print(f'Rust notices: {len(records)} components; all release/build dependencies resolved for {target}.')
    if unresolved:
        print('Non-release license follow-up required before enabling these dependencies: ' + ', '.join(unresolved))
    return output


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', help='Release Rust target triple; defaults to the host target')
    parser.add_argument('--offline', action='store_true', help='Require every supplemental notice in the verified cache')
    options = parser.parse_args()
    build(options.target, options.offline)
