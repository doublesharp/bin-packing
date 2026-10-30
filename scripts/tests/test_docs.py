"""Documentation checks must catch drift and broken navigation before publishing."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('check_docs', Path(__file__).parents[1] / 'check-docs.py')
DOCS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DOCS)


class DocumentationTests(unittest.TestCase):
    def test_rust_example_project_has_sources_before_cargo_resolves_the_lock(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'README.md').write_text('```rust\nfn main() {}\n```\n')
            (root / 'Cargo.lock').write_text('version=4\n[[package]]\nname="bin-packing"\nversion="0.6.0"\n')

            def cargo(command, **_options):
                manifest = Path(command[command.index('--manifest-path') + 1])
                self.assertTrue(list((manifest.parent / 'src/bin').glob('*.rs')),
                                'Cargo metadata must see runnable example sources')

            with patch.object(DOCS.subprocess, 'run', side_effect=cargo):
                DOCS.rust_examples(root)

    def test_link_checker_rejects_missing_files_and_anchors_but_ignores_examples(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'README.md').write_text(
                '[bad](missing.md)\n[anchor](guide.md#missing)\n'
                '```md\n[illustration](not-a-real-link.md)\n```\n')
            (root / 'guide.md').write_text('# Existing heading\n')
            errors = DOCS.local_link_errors(root)
            self.assertEqual(len(errors), 2)
            self.assertTrue(any('missing.md' in error for error in errors))
            self.assertTrue(any('#missing' in error for error in errors))
            (root / 'README.md').write_text('[valid](guide.md#existing-heading)\n')
            self.assertEqual(DOCS.local_link_errors(root), [])

    def test_reference_links_encoded_paths_and_duplicate_headings(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'README.md').write_text(
                '# Same\n# Same\n[local](#same-1)\n'
                '[guide]: guide%20name.md#api\n[remote](https://example.com/missing)\n')
            (root / 'guide name.md').write_text('# `API`\n```sh\n# fake heading\n```\n')
            self.assertEqual(DOCS.local_link_errors(root), [])
            self.assertNotIn('fake-heading', DOCS.headings((root / 'guide name.md').read_text()))

    def test_dependency_update_preserves_prose_and_rejects_missing_markers(self):
        text = 'before\n' + DOCS.dependency_block('0.5.0') + '\nafter\n'
        expected = 'before\n' + DOCS.dependency_block('0.6.0') + '\nafter\n'
        self.assertEqual(DOCS.update_dependency(text, '0.6.0'), expected)
        with self.assertRaises(ValueError):
            DOCS.update_dependency('bin-packing = "0.5.0"', '0.6.0')

    def test_current_release_and_main_github_links_are_checked_as_local_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.6.0"\n')
            prefix = 'https://github.com/doublesharp/bin-packing/blob/'
            (root / 'README.md').write_text(
                f'[main]({prefix}main/missing.md)\n[release]({prefix}v0.6.0/guide.md#missing)\n')
            (root / 'guide.md').write_text('# Existing\n')
            self.assertEqual(len(DOCS.local_link_errors(root)), 2)
            (root / 'README.md').write_text(f'[valid]({prefix}v0.6.0/guide.md#existing)\n')
            self.assertEqual(DOCS.local_link_errors(root), [])

    def test_reference_follows_versions_exports_features_and_real_default_values(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            files = {
                'Cargo.toml': '[workspace.package]\nversion="0.6.0"\nrust-version="1.99"\n',
                'rust-toolchain.toml': '[toolchain]\nchannel="1.99.0"\n',
                '.github/workflows/ci.yml': 'env:\n  DOCS_TYPESCRIPT_VERSION: "5.9.3"\n  DOCS_PLAYWRIGHT_VERSION: "1.56.1"\n  NODE_VERSION: "22"\n',
                'crates/bin-packing/Cargo.toml': '[features]\ndefault=["one-d"]\none-d=[]\n',
                'bindings/node/package.json': json.dumps({'engines': {'node': '>=18'}, 'napi': {'targets': ['aarch64-apple-darwin']}}),
                'bindings/wasm/package.json': json.dumps({'name': '@example/packing', 'engines': {'node': '>=18'}, 'exports': {'.': {'types': './index.d.ts'}}}),
            }
            for name in ['node/wrapper', 'wasm/types/bin_packing_wasm', 'wasm/types/one_d',
                         'wasm/types/two_d', 'wasm/types/three_d', 'wasm/types/irregular']:
                files[f'bindings/{name}.d.ts'] = 'export function version(): string;\n'
            for name, text in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            before = DOCS.render_reference(root, {'Options': {'beam_width': 8}})
            (root / 'Cargo.toml').write_text(files['Cargo.toml'].replace('0.6.0', '0.7.0'))
            with (root / 'bindings/wasm/types/irregular.d.ts').open('a') as output:
                output.write('export function nest(): unknown;\n')
            after = DOCS.render_reference(root, {'Options': {'beam_width': 16}})
            self.assertNotEqual(before, after)
            self.assertIn('`0.7.0`', after)
            self.assertIn('| `nest` | — | — | — | — | — | Yes |', after)
            self.assertIn('"beam_width": 16', after)


if __name__ == '__main__':
    unittest.main()
