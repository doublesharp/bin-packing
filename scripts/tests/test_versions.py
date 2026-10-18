"""Keep source packages and release metadata on the same version."""
import json
from pathlib import Path
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]


def toml(path):
    return tomllib.loads((ROOT / path).read_text())


class VersionTests(unittest.TestCase):
    def test_npm_and_rust_packages_match_workspace(self):
        version = toml('Cargo.toml')['workspace']['package']['version']
        for directory in ['bindings/node', 'bindings/wasm']:
            with self.subTest(directory=directory):
                metadata = json.loads((ROOT / directory / 'package.json').read_text())
                self.assertEqual(metadata['version'], version)
                self.assertTrue(toml(directory + '/Cargo.toml')['package']['version']['workspace'])
        self.assertTrue(toml('crates/bin-packing/Cargo.toml')['package']['version']['workspace'])

    def test_local_lockfile_packages_match_workspace(self):
        version = toml('Cargo.toml')['workspace']['package']['version']
        for path, names in [('Cargo.lock', {'bin-packing', 'bin-packing-node', 'bin-packing-wasm'}),
                            ('fuzz/Cargo.lock', {'bin-packing'})]:
            with self.subTest(path=path):
                local = {p['name']: p for p in toml(path)['package'] if p['name'] in names}
                self.assertEqual(set(local), names)
                for package in local.values():
                    self.assertEqual(package['version'], version)
                    self.assertNotIn('source', package)

    def test_current_version_has_release_notes(self):
        version = toml('Cargo.toml')['workspace']['package']['version']
        path = ROOT / 'docs' / 'releases' / f'v{version}.md'
        self.assertTrue(path.is_file(), f'Missing release notes: {path.name}')
        self.assertEqual(path.read_text().splitlines()[0], f'# v{version}')
