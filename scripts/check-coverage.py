#!/usr/bin/env python3
"""Measure production Rust coverage without counting test code or changing sources."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
CHARACTER = re.compile(r"'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\n])|[^\\'\n])'")
RAW_STRING = re.compile(r'(?:br|cr|r)(#{0,255})"')


def code_characters(text, start=0):
    """Yield original offsets outside Rust literals and nested comments."""
    comments, index = 0, start
    while index < len(text):
        if comments:
            if text.startswith('/*', index):
                comments += 1
                index += 2
            elif text.startswith('*/', index):
                comments -= 1
                index += 2
            else:
                index += 1
            continue
        if text.startswith('//', index):
            end = text.find('\n', index)
            index = len(text) if end < 0 else end
            continue
        if text.startswith('/*', index):
            comments = 1
            index += 2
            continue
        raw = RAW_STRING.match(text, index)
        if raw:
            closing = '"' + raw.group(1)
            end = text.find(closing, raw.end())
            if end < 0:
                raise ValueError('unterminated raw string in test item')
            index = end + len(closing)
            continue
        if text[index] == '"':
            index += 1
            while index < len(text) and text[index] != '"':
                index += 2 if text[index] == '\\' else 1
            if index >= len(text):
                raise ValueError('unterminated string in test item')
            index += 1
            continue
        character = CHARACTER.match(text, index)
        if character:
            index = character.end()
            continue
        yield index, text[index]
        index += 1
    if comments:
        raise ValueError('unterminated block comment')


def block_end(text, start):
    """Locate a Rust item's closing brace, ignoring literals and nested comments."""
    depth, opened = 0, False
    for index, character in code_characters(text, start):
        if character == '{':
            opened = True
            depth += 1
        elif character == '}':
            depth -= 1
            if opened and depth == 0:
                return index
    raise ValueError('unterminated test item')


def test_scope(core):
    """Find formatted top-level cfg(test) items and their external module trees."""
    ranges, external = {}, set()
    for path in core.rglob('*.rs'):
        text = path.read_text()
        masked = ['\n' if character == '\n' else ' ' for character in text]
        for index, character in code_characters(text):
            masked[index] = character
        code = ''.join(masked)
        spans = []
        for marker in re.finditer(r'^([ \t]*)#\[cfg\(test\)\][ \t]*\n', code, re.MULTILINE):
            if marker.group(1):
                raise ValueError(f'nested cfg(test) item needs explicit handling: {path}')
            item = re.match(r'(?:pub(?:\([^)]*\))? )?(mod|fn) (\w+)', code[marker.end():])
            if not item:
                raise ValueError(f'unrecognized cfg(test) item: {path}')
            start = text.count('\n', 0, marker.start()) + 1
            module = re.match(r'(?:pub(?:\([^)]*\))? )?mod (\w+);', code[marker.end():])
            if module:
                parent = path.parent if path.name in {'mod.rs', 'lib.rs'} else path.with_suffix('')
                module_path = parent / module.group(1)
                external.update({module_path.with_suffix('.rs'), module_path})
                end = text.count('\n', 0, marker.end() + module.end()) + 1
            else:
                end = text.count('\n', 0, block_end(text, marker.end())) + 1
            spans.append((start, end))
        ranges[path.resolve()] = spans
    return ranges, external


def summarize_lcov(text, workspace):
    """Count unique LCOV source lines and branch outcomes in production library code."""
    core = (workspace / 'crates/bin-packing/src').resolve()
    ranges, external = test_scope(core)
    files, seen = [], set()
    for record in text.split('end_of_record'):
        rows = record.strip().splitlines()
        if not rows:
            continue
        filenames = [row[3:] for row in rows if row.startswith('SF:')]
        if len(filenames) != 1:
            raise ValueError('LCOV record must have exactly one source filename')
        path = Path(filenames[0]).resolve()
        if not path.is_relative_to(core) or any(path == item or path.is_relative_to(item) for item in external):
            continue
        if path in seen:
            raise ValueError(f'duplicate source record: {path}')
        seen.add(path)
        if path not in ranges:
            raise ValueError(f'coverage source is missing: {path}')
        lines, branches = {}, {}
        for row in rows:
            if row.startswith('DA:'):
                line, count, *_ = row[3:].split(',')
                key, count = int(line), int(count)
                values = lines
            elif row.startswith('BRDA:'):
                line, block, branch, count = row[5:].split(',')
                key = (int(line), block, branch)
                count = 0 if count == '-' else int(count)
                values = branches
            else:
                continue
            if int(line) <= 0 or count < 0 or key in values:
                raise ValueError(f'invalid or duplicate coverage counter: {path}: {row}')
            if not any(start <= int(line) <= end for start, end in ranges[path]):
                values[key] = count
        files.append({
            'file': str(path.relative_to(workspace)),
            'lines': len(lines), 'lines_hit': sum(value > 0 for value in lines.values()),
            'branches': len(branches), 'branches_hit': sum(value > 0 for value in branches.values()),
            'missing_lines': [line for line, count in lines.items() if count == 0],
            'missing_branches': [list(key) for key, count in branches.items() if count == 0],
            'excluded_test_ranges': ranges[path],
        })
    totals = {key: sum(item[key] for item in files) for key in ['lines', 'lines_hit', 'branches', 'branches_hit']}
    if not totals['lines'] or not totals['branches']:
        raise ValueError('production line and branch coverage data are required')
    totals['line_percent'] = 100 * totals['lines_hit'] / totals['lines']
    totals['branch_percent'] = 100 * totals['branches_hit'] / totals['branches']
    return {'production_core': totals, 'files': files,
            'method': 'Unique LLVM LCOV DA and BRDA records; cfg(test) items and external test modules excluded; no production code excluded.'}


def executable_artifacts(log):
    """Read exact executables from Cargo JSON, independent of target directory layout."""
    artifacts = set()
    for line in log.splitlines():
        if not line.startswith('{'):
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (message.get('reason') == 'compiler-artifact' and message.get('executable')
                and 'custom-build' not in message.get('target', {}).get('kind', [])):
            artifacts.add(message['executable'])
    if not artifacts:
        raise ValueError('Cargo reported no executable test artifacts')
    return sorted(artifacts)


def run(command, log, env):
    with log.open('w') as output:
        result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, env=env, check=False)
    if result.returncode:
        raise RuntimeError(f'command exited {result.returncode}; see {log}')


def measure(args):
    output, target = args.output_dir.resolve(), args.target_dir.resolve()
    original = Path(os.environ.get('BIN_PACKING_SOURCE', str(ROOT))).resolve()
    if any(path.is_relative_to(root) for path in [output, target] for root in [ROOT, original]):
        raise ValueError('coverage output and target directories must be outside the source checkout')
    if sys.platform == 'darwin':
        scratch = Path('/Volumes/Scratch')
        if not os.path.ismount(scratch):
            raise ValueError('Scratch must be mounted before writing coverage output')
        if not all(path.is_relative_to(scratch) for path in [output, target]):
            raise ValueError('use Scratch for coverage output and target directories on macOS')
    if output == target or output.is_relative_to(target) or target.is_relative_to(output):
        raise ValueError('coverage output and target directories must be separate')
    if output.exists() and any(output.iterdir()):
        raise ValueError('use a fresh coverage output directory; existing receipts are preserved')
    output.mkdir(parents=True, exist_ok=True)
    target.mkdir(parents=True, exist_ok=True)
    profiles = output / 'profiles'
    profiles.mkdir()
    temporary = target / 'tmp'
    temporary.mkdir(exist_ok=True)
    env = dict(os.environ, CARGO_TARGET_DIR=str(target), RUSTC_WRAPPER='',
               LLVM_PROFILE_FILE=str(profiles / '%m-%p.profraw'),
               TMPDIR=str(temporary), TMP=str(temporary), TEMP=str(temporary))
    env['RUSTFLAGS'] = env.get('RUSTFLAGS', '') + ' -C instrument-coverage -Z coverage-options=branch'
    toolchain = subprocess.check_output(['rustc', '+' + args.toolchain, '--print', 'sysroot'], text=True, env=env).strip()
    version = subprocess.check_output(['rustc', '+' + args.toolchain, '-vV'], text=True, env=env)
    host = next(line.removeprefix('host: ') for line in version.splitlines() if line.startswith('host: '))
    llvm = Path(toolchain) / 'lib/rustlib' / host / 'bin'
    for name in ['llvm-cov', 'llvm-profdata']:
        if not (llvm / name).is_file():
            raise ValueError(f'{name} missing; install llvm-tools for {args.toolchain}')
    sources = sorted(ROOT.joinpath('crates/bin-packing/src').rglob('*.rs'))
    before = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
    command = ['cargo', '+' + args.toolchain, 'test', '--workspace', '--all-targets',
               '--all-features', '--locked', '--jobs', str(args.jobs), '--message-format=json']
    run(command, output / 'tests.log', env)
    artifacts = executable_artifacts((output / 'tests.log').read_text())
    raw = sorted(profiles.glob('*.profraw'))
    if not raw:
        raise ValueError('instrumented tests produced no raw profiles')
    profile = output / 'coverage.profdata'
    run([str(llvm / 'llvm-profdata'), 'merge', '-sparse', *map(str, raw), '-o', str(profile)], output / 'merge.log', env)
    export = [str(llvm / 'llvm-cov'), 'export', artifacts[0], '--instr-profile=' + str(profile),
              '--include-filename-regex=' + re.escape(str(ROOT / 'crates/bin-packing/src')) + '/',
              *['--object=' + artifact for artifact in artifacts[1:]]]
    for kind, name in [('lcov', 'coverage.lcov'), ('text', 'coverage.json')]:
        with (output / name).open('w') as data, (output / (name + '.stderr')).open('w') as errors:
            result = subprocess.run([*export, '--format=' + kind], stdout=data, stderr=errors,
                                    env=env, check=False)
        if result.returncode:
            raise RuntimeError(f'coverage export exited {result.returncode}; see {output / (name + ".stderr")}')
    summary = summarize_lcov((output / 'coverage.lcov').read_text(), ROOT)
    after = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}
    if before != after:
        raise ValueError('Rust library source changed during measurement')
    summary.update({'toolchain': version.strip(), 'command': command, 'objects': artifacts,
                    'source_sha256': before, 'source_unchanged': True,
                    'scope': 'Production Rust library, all features and Rust test targets; JS/browser execution and doctests excluded.'})
    (output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    totals = summary['production_core']
    print(f'Production lines: {totals["line_percent"]:.2f}% ({totals["lines_hit"]}/{totals["lines"]})')
    print(f'Production branches: {totals["branch_percent"]:.2f}% ({totals["branches_hit"]}/{totals["branches"]})')
    print(f'Report: {output / "summary.json"}')
    if totals['line_percent'] < args.min_lines or totals['branch_percent'] < args.min_branches:
        raise ValueError('coverage is below the requested minimum')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output-dir', required=True, type=Path)
    parser.add_argument('--target-dir', required=True, type=Path)
    parser.add_argument('--toolchain', default='nightly-2026-10-12')
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--min-lines', type=float, default=0)
    parser.add_argument('--min-branches', type=float, default=0)
    args = parser.parse_args()
    if args.jobs < 1 or not 0 <= args.min_lines <= 100 or not 0 <= args.min_branches <= 100:
        parser.error('jobs must be positive and coverage minimums must be between 0 and 100')
    if (ROOT / '.scratch.json').exists() and os.environ.get('BIN_PACKING_SCRATCH_ACTIVE') != '1':
        return subprocess.run([sys.executable, '-B', str(ROOT / 'scripts/scratch.py'), 'run',
                               sys.executable, '-B', 'scripts/check-coverage.py', *sys.argv[1:]],
                              cwd=ROOT, check=False).returncode
    try:
        measure(args)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'error: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
