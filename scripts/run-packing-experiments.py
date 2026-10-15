#!/usr/bin/env python3
"""Reproduce targeted, serial release measurements in two isolated checkouts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess


BASE = '0b626ac920d2ef5845c5e1763a2999ee5baf0c29'
EXAMPLE = Path('crates/bin-packing/examples/packing_experiment_bench.rs')
FIXTURES = ['pattern-count.json', 'pattern-offcuts.json', 'arc-auto.json']


def git_revision(root):
    return subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=root, check=True,
                          capture_output=True, text=True).stdout.strip()


def prepare_baseline(baseline, candidate):
    if git_revision(baseline) != BASE:
        raise ValueError('Baseline checkout must be pinned to ' + BASE)
    changed = subprocess.run(['git', 'diff', '--name-only'], cwd=baseline,
                             check=True, capture_output=True, text=True).stdout.splitlines()
    if any(path != 'crates/bin-packing/Cargo.toml' for path in changed):
        raise ValueError('Baseline has unrelated tracked changes')
    text = (candidate / EXAMPLE).read_text()
    text, count = re.subn(r'    // EXPERIMENT_ONLY_START.*?    // EXPERIMENT_ONLY_END\n',
                         '', text, flags=re.DOTALL)
    if count != 1:
        raise ValueError('Expected one optional-API benchmark block')
    (baseline / EXAMPLE).parent.mkdir(parents=True, exist_ok=True)
    (baseline / EXAMPLE).write_text(text)
    for name in FIXTURES:
        relative = Path('crates/bin-packing/tests/fixtures') / name
        (baseline / relative).parent.mkdir(parents=True, exist_ok=True)
        (baseline / relative).write_bytes((candidate / relative).read_bytes())
    manifest = baseline / 'crates/bin-packing/Cargo.toml'
    text = manifest.read_text()
    if 'name = "packing_experiment_bench"' not in text:
        text += ('\n[[example]]\nname = "packing_experiment_bench"\n'
                 'required-features = ["one-d", "two-d", "three-d"]\n')
        manifest.write_text(text)


def run(root, name, folder, environment):
    print('Building ' + name + ' release harness', flush=True)
    build = subprocess.run(['cargo', 'build', '-p', 'bin-packing', '--release',
                            '--example', 'packing_experiment_bench', '--locked', '-j', '2'],
                           cwd=root, env=environment, capture_output=True, text=True)
    (folder / (name + '-build.log')).write_text(build.stdout + build.stderr)
    build.check_returncode()
    config = json.loads((root / '.scratch.json').read_text())
    binary = Path(config['root']) / 'target/release/examples/packing_experiment_bench'
    print('Measuring ' + name + ' serially', flush=True)
    result = subprocess.run([str(binary)], cwd=root, env=environment, check=True,
                            capture_output=True, text=True)
    rows = json.loads(result.stdout)
    for row in rows:
        canonical = json.dumps(row['input'], sort_keys=True, separators=(',', ':'))
        row['input_sha256'] = hashlib.sha256(canonical.encode()).hexdigest()
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', required=True, type=Path)
    parser.add_argument('--candidate', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    baseline, candidate = args.baseline.resolve(), args.candidate.resolve()
    if baseline == candidate:
        raise ValueError('Baseline and candidate must be different isolated checkouts')
    folder = args.output.resolve().parent
    folder.mkdir(parents=True, exist_ok=True)
    prepare_baseline(baseline, candidate)
    environment = dict(os.environ, RAYON_NUM_THREADS='1')
    before = run(baseline, 'baseline', folder, environment)
    after = run(candidate, 'candidate', folder, environment)
    matched = {row['case']: row for row in after if row['variant'] == 'ordinary'}
    for row in before:
        if row['input_sha256'] != matched[row['case']]['input_sha256']:
            raise ValueError('Mismatched before/after input for ' + row['case'])
    rust = subprocess.run(['rustc', '--version'], check=True,
                          capture_output=True, text=True).stdout.strip()
    output = {
        'scope': 'Targeted regression fixtures, not a representative performance claim',
        'method': 'One untimed warmup; serial release samples; includes input cloning, excludes serialization and compilation',
        'hardware': {'platform': platform.platform(), 'machine': platform.machine(),
                     'logical_cpus': os.cpu_count(), 'rayon_threads': 1, 'rustc': rust},
        'baseline_commit': git_revision(baseline), 'candidate_commit': git_revision(candidate),
        'baseline': before, 'candidate': after,
    }
    args.output.write_text(json.dumps(output, indent=2) + '\n')
    print('Wrote ' + str(args.output), flush=True)


if __name__ == '__main__':
    main()
