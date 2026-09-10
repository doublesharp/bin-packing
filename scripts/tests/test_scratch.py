"""Behavioral checks for isolated disposable build workspaces."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'scratch.py'


class ScratchTests(unittest.TestCase):
    def test_runner_preserves_arguments_sources_and_exit_status(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            source = base / 'source with spaces'
            source.mkdir()
            subprocess.run(['git', 'init', '-q', str(source)], check=True)
            (source / 'kept.txt').write_text('original')
            (source / '.gitignore').write_text('ignored\n.scratch.json\n.cargo/config.toml\n')
            (source / 'ignored').write_text('do not copy')
            script_spec = importlib.util.spec_from_file_location('scratch', SCRIPT)
            module = importlib.util.module_from_spec(script_spec)
            script_spec.loader.exec_module(module)
            scratch = base / 'scratch'
            scratch.mkdir()
            # Use the temporary filesystem's real mount point, avoiding a fake mounted volume.
            mount = base
            while not os.path.ismount(mount):
                mount = mount.parent
            module.setup(source, mount, scratch)
            module.setup(source, mount, scratch)  # Setup must be idempotent.
            with self.assertRaisesRegex(ValueError, 'inside the build workspace'):
                module.run(source, ['true'], '..')
            config = json.loads((source / '.scratch.json').read_text())
            command = ['python3', '-c', 'import os,sys;from pathlib import Path;assert sys.argv[1]=="a b;$x";assert not Path("ignored").exists();assert Path(os.environ["TMPDIR"]).is_dir();assert all(Path(os.environ[key]).is_relative_to(Path(os.environ["TMPDIR"]).parent) for key in ["CARGO_HOME","CARGO_TARGET_DIR","npm_config_cache","npm_config_store_dir","WASM_PACK_CACHE","COREPACK_HOME"]);Path("kept.txt").write_text("build copy");sys.exit(7)', 'a b;$x']
            self.assertEqual(module.run(source, command, '.'), 7)
            self.assertEqual((source / 'kept.txt').read_text(), 'original')
            workspace = Path(config['root']) / 'workspace'
            self.assertEqual((workspace / 'kept.txt').read_text(), 'build copy')
            (source / 'kept.txt').unlink()
            (workspace / 'generated').write_text('retain output')
            self.assertEqual(module.run(source, ['python3', '-c', 'from pathlib import Path;assert not Path("kept.txt").exists();assert Path("generated").exists()'], '.'), 0)
            # Missing mounted volume must not fall back to source-volume builds.
            config['volume'] = str(base / 'missing')
            (source / '.scratch.json').write_text(json.dumps(config))
            with self.assertRaisesRegex(ValueError, 'mounted'):
                module.run(source, ['true'], '.')

    def test_setup_preserves_existing_cargo_configuration(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary)
            (source / '.cargo').mkdir()
            config = source / '.cargo/config.toml'
            config.write_text('[build]\njobs = 2\n')
            spec = importlib.util.spec_from_file_location('scratch', SCRIPT)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            with self.assertRaisesRegex(ValueError, 'existing Cargo'):
                module.setup(source, Path('/'), source / 'scratch')
            self.assertEqual(config.read_text(), '[build]\njobs = 2\n')


if __name__ == '__main__':
    unittest.main()
