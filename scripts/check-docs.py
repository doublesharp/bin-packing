#!/usr/bin/env python3
"""Generate source-derived documentation, check local links and run Rust examples."""
import argparse
import difflib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
BEGIN = '<!-- BEGIN GENERATED: dependency -->'
END = '<!-- END GENERATED: dependency -->'


def markdown_files(root):
    """Visit maintained and historical docs, excluding build and dependency trees."""
    return sorted(set(root.glob('*.md')) | set(root.glob('bindings/*/README.md'))
                  | set(root.glob('docs/**/*.md')))


def headings(text):
    """GitHub-style heading anchors, including suffixes for duplicate headings."""
    text = re.sub(r'^```[^\n]*\n.*?^```\s*$', '', text, flags=re.M | re.S)
    anchors, counts = set(), {}
    for title in re.findall(r'^#{1,6}\s+(.+?)\s*#*$', text, re.M):
        title = re.sub(r'\[([^]]+)\]\([^)]*\)', r'\1', title)
        slug = re.sub(r'[^\w\- ]', '', title.lower()).replace(' ', '-')
        count = counts.get(slug, 0)
        anchors.add(f'{slug}-{count}' if count else slug)
        counts[slug] = count + 1
    return anchors


def local_link_errors(root):
    """Check inline/reference links and Markdown anchors without fetching the web."""
    errors = []
    cargo = root / 'Cargo.toml'
    version = tomllib.loads(cargo.read_text())['workspace']['package']['version'] if cargo.exists() else None
    for page in markdown_files(root):
        text = page.read_text()
        # Fenced source examples can contain Markdown-like text that is not a link.
        prose = re.sub(r'^```[^\n]*\n.*?^```\s*$', '', text, flags=re.M | re.S)
        links = re.findall(r'!?\[[^]\n]*\]\(([^\s)]+)(?:\s+"[^"]*")?\)', prose)
        links += re.findall(r'^\[[^]]+\]:\s*(\S+)', prose, re.M)
        for raw in links:
            target = urlsplit(raw.strip('<>'))
            own = re.fullmatch(r'/doublesharp/bin-packing/blob/(main|v\d+\.\d+\.\d+)/(.*)', target.path)
            if target.netloc == 'github.com' and own and own[1] in {'main', f'v{version}'}:
                target = target._replace(scheme='', netloc='', path=own[2])
                destination = root / unquote(target.path)
            else:
                destination = page.parent / unquote(target.path) if target.path else page
            if target.scheme or target.netloc:
                continue
            if not destination.exists():
                errors.append(f'{page.relative_to(root)}: missing {raw}')
            elif target.fragment and destination.suffix == '.md':
                anchor = unquote(target.fragment)
                if anchor not in headings(destination.read_text()):
                    errors.append(f'{page.relative_to(root)}: missing anchor {raw}')
    return errors


def functions(path):
    return set(re.findall(r'^export (?:declare )?function (\w+)\(', path.read_text(), re.M))


def rust_defaults(root):
    command = ['cargo', 'run', '--quiet', '-p', 'bin-packing', '--example',
               'doc_reference', '--all-features', '--locked']
    if (root / '.scratch.json').exists() and os.environ.get('BIN_PACKING_SCRATCH_ACTIVE') != '1':
        command = [sys.executable, '-B', 'scripts/scratch.py', 'run', *command]
    output = subprocess.check_output(command, cwd=root, text=True)
    # The Scratch wrapper writes its build location before the JSON line.
    return json.loads(output.splitlines()[-1])


def dependency_block(version):
    return f'{BEGIN}\n```toml\n[dependencies]\nbin-packing = "{version}"\n```\n{END}'


def update_dependency(text, version):
    if text.count(BEGIN) != 1 or text.count(END) != 1:
        raise ValueError('README dependency markers must each occur exactly once')
    return re.sub(re.escape(BEGIN) + r'.*?' + re.escape(END),
                  lambda _: dependency_block(version), text, flags=re.S)


def wasm_import_table(wasm):
    lines = ['| Import | Target / initialization |', '| --- | --- |']
    for key, value in wasm['exports'].items():
        if not isinstance(value, dict) or 'types' not in value:
            continue
        path = wasm['name'] + ('' if key == '.' else key[1:])
        flavor = 'web: await init()' if key.endswith('/web') else (
            'nodejs: synchronous' if key.endswith('/nodejs') else 'bundler: WASM ESM support')
        lines.append(f'| `{path}` | {flavor} |')
    return '\n'.join(lines)


def render_reference(root, defaults):
    workspace = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']
    core = tomllib.loads((root / 'crates/bin-packing/Cargo.toml').read_text())
    toolchain = tomllib.loads((root / 'rust-toolchain.toml').read_text())['toolchain']
    node = json.loads((root / 'bindings/node/package.json').read_text())
    wasm = json.loads((root / 'bindings/wasm/package.json').read_text())
    workflow = (root / '.github/workflows/ci.yml').read_text()
    ci_tools = {}
    for name in ['DOCS_TYPESCRIPT_VERSION', 'DOCS_PLAYWRIGHT_VERSION', 'NODE_VERSION']:
        value = re.search(r'^  ' + name + r': "([^"]+)"$', workflow, re.M)
        if value is None:
            raise ValueError(f'Missing explicit CI tool version: {name}')
        ci_tools[name] = value[1]
    lines = ['# Generated package reference', '',
             '<!-- Generated by scripts/check-docs.py --write. Do not edit by hand. -->', '',
             'This page records source configuration, not runtime or platform qualification.', '',
             '## Release and tools', '',
             '| Fact | Source value |', '| --- | --- |',
             f'| Workspace / npm version | `{workspace["version"]}` |',
             f'| Release notes | [v{workspace["version"]}](releases/v{workspace["version"]}.md) |',
             f'| Minimum Rust version | `{workspace["rust-version"]}` |',
             f'| Pinned Rust toolchain | `{toolchain["channel"]}` |',
             f'| Native Node engine | `{node["engines"]["node"]}` |',
             f'| WASM Node engine | `{wasm["engines"]["node"]}` |',
             f'| CI Node version | `{ci_tools["NODE_VERSION"]}` |',
             f'| CI TypeScript | `{ci_tools["DOCS_TYPESCRIPT_VERSION"]}` |',
             f'| CI Playwright | `{ci_tools["DOCS_PLAYWRIGHT_VERSION"]}` |', '',
             '## Cargo features', '', '| Feature | Enables |', '| --- | --- |']
    for name, values in core['features'].items():
        lines.append(f'| `{name}` | ' + (', '.join(f'`{value}`' for value in values) or 'No additional features') + ' |')
    lines += ['', '## Native package targets', '']
    lines += [f'- `{target}`' for target in node['napi']['targets']]
    lines += ['', '## WASM imports', '', wasm_import_table(wasm)]
    sources = [('Native', 'bindings/node/wrapper.d.ts'),
               ('Combined WASM', 'bindings/wasm/types/bin_packing_wasm.d.ts'),
               ('1D WASM', 'bindings/wasm/types/one_d.d.ts'),
               ('2D WASM', 'bindings/wasm/types/two_d.d.ts'),
               ('3D WASM', 'bindings/wasm/types/three_d.d.ts'),
               ('Irregular WASM', 'bindings/wasm/types/irregular.d.ts')]
    exports = [(name, functions(root / path)) for name, path in sources]
    lines += ['', '## Declared object and JSON APIs', '',
              'Web initialization helpers are additional to the functions below.', '',
              '| Function | ' + ' | '.join(name for name, _ in exports) + ' |',
              '| --- | ' + ' | '.join('---' for _ in exports) + ' |']
    for name in sorted(set().union(*(names for _, names in exports))):
        lines.append(f'| `{name}` | ' + ' | '.join('Yes' if name in names else '—' for _, names in exports) + ' |')
    lines += ['', '## Serialized Rust defaults', '',
              'Produced by the `doc_reference` example from actual `Default` implementations.',
              'Seeds shown as `null` use each solver\'s documented fallback. These are defaults,',
              'not maximum work limits; see each model guide for bounds.', '']
    for name, value in sorted(defaults.items()):
        lines += [f'### {name}', '', '```json', json.dumps(value, indent=2, sort_keys=True), '```', '']
    return '\n'.join(lines)


def generated_files(root, defaults):
    version = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']['version']
    readme = update_dependency((root / 'README.md').read_text(), version)
    readme = re.sub(r'\[v\d+\.\d+\.\d+ release notes\]\(docs/releases/v\d+\.\d+\.\d+\.md\)',
                    f'[v{version} release notes](docs/releases/v{version}.md)', readme)
    index = re.sub(r'\[v\d+\.\d+\.\d+\]\(releases/v\d+\.\d+\.\d+\.md\)',
                   f'[v{version}](releases/v{version}.md)', (root / 'docs/README.md').read_text())
    files = {root / 'docs/reference.md': render_reference(root, defaults),
             root / 'README.md': readme, root / 'docs/README.md': index}
    for name in ['node', 'wasm']:
        page = root / f'bindings/{name}/README.md'
        files[page] = re.sub(r'https://github.com/doublesharp/bin-packing/blob/(?:main|v\d+\.\d+\.\d+)/',
                             f'https://github.com/doublesharp/bin-packing/blob/v{version}/', page.read_text())
        if name == 'wasm':
            begin, end = '<!-- BEGIN GENERATED: wasm-imports -->', '<!-- END GENERATED: wasm-imports -->'
            if files[page].count(begin) != 1 or files[page].count(end) != 1:
                raise ValueError('WASM README import markers must each occur exactly once')
            wasm = json.loads((root / 'bindings/wasm/package.json').read_text())
            files[page] = re.sub(re.escape(begin) + r'.*?' + re.escape(end),
                                 lambda _: begin + '\n' + wasm_import_table(wasm) + '\n' + end,
                                 files[page], flags=re.S)
    return files


def rust_examples(root):
    """Compile and execute complete Rust examples; partial snippets stay illustrative."""
    examples = []
    for page in markdown_files(root):
        if any(part in {'experiments', 'superpowers', 'releases'} for part in page.relative_to(root).parts):
            continue
        for block in re.findall(r'^```rust\n(.*?)^```', page.read_text(), re.M | re.S):
            if re.search(r'^fn main\(', block, re.M):
                examples.append((page.relative_to(root), block))
    if not examples:
        raise ValueError('No complete runnable Rust documentation examples found')
    with tempfile.TemporaryDirectory(prefix='bin-packing-docs-') as temporary:
        project = Path(temporary)
        (project / 'src/bin').mkdir(parents=True)
        (project / 'Cargo.toml').write_text(
            '[package]\nname = "bin-packing-doc-examples"\nversion = "0.0.0"\nedition = "2024"\n'
            '[workspace]\n[dependencies]\nbin-packing = { path = '
            + json.dumps(str(root / 'crates/bin-packing')) + ' }\n')
        for index, (_page, block) in enumerate(examples):
            (project / f'src/bin/example_{index}.rs').write_text(block)
        # Reuse the workspace resolution; only the temporary root package is new.
        (project / 'Cargo.lock').write_text((root / 'Cargo.lock').read_text())
        subprocess.run(['cargo', 'check', '--quiet', '--offline',
                        '--manifest-path', str(project / 'Cargo.toml')], check=True)
        original = tomllib.loads((root / 'Cargo.lock').read_text())['package']
        resolved = tomllib.loads((project / 'Cargo.lock').read_text())['package']
        locked = {(p['name'], p['version'], p.get('source')) for p in original}
        for package in resolved:
            identity = (package['name'], package['version'], package.get('source'))
            if package['name'] != 'bin-packing-doc-examples' and identity not in locked:
                raise ValueError(f'Documentation example resolved outside the workspace lock: {identity}')
        for index, (page, _block) in enumerate(examples):
            print(f'Rust documentation example {index}: {page}', flush=True)
            subprocess.run(['cargo', 'run', '--quiet', '--offline', '--locked', '--manifest-path',
                            str(project / 'Cargo.toml'), '--bin', f'example_{index}'], check=True)
    print(f'Executed {len(examples)} complete Rust documentation examples.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='Update generated facts in the original checkout')
    parser.add_argument('--rust-examples', action='store_true', help='Also compile and run complete Rust examples')
    args = parser.parse_args()
    if args.write and os.environ.get('BIN_PACKING_SOURCE'):
        parser.error('--write must run in the original checkout, outside the Scratch mirror')
    if args.rust_examples and (ROOT / '.scratch.json').exists() and os.environ.get('BIN_PACKING_SCRATCH_ACTIVE') != '1':
        if args.write:
            parser.error('Run --write and --rust-examples separately')
        return subprocess.run([sys.executable, '-B', 'scripts/scratch.py', 'run',
                               sys.executable, '-B', 'scripts/check-docs.py', '--rust-examples'], cwd=ROOT, check=False).returncode
    stale = []
    for path, expected in generated_files(ROOT, rust_defaults(ROOT)).items():
        actual = path.read_text() if path.exists() else ''
        if actual != expected:
            if args.write:
                path.write_text(expected)
                print(f'Updated {path.relative_to(ROOT)}')
            else:
                stale.append(str(path.relative_to(ROOT)))
                sys.stderr.writelines(difflib.unified_diff(actual.splitlines(True), expected.splitlines(True),
                                                         fromfile=str(path), tofile='expected'))
    errors = local_link_errors(ROOT)
    if stale:
        errors.append('Stale generated docs: ' + ', '.join(stale) + '. Run python3 -B scripts/check-docs.py --write.')
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    if args.rust_examples:
        rust_examples(ROOT)
    print('Generated documentation and local links passed.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
