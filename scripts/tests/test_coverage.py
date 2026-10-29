"""Check that coverage reporting cannot gain credit from test-only source."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('coverage_report', Path(__file__).resolve().parents[1] / 'check-coverage.py')
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class CoverageTests(unittest.TestCase):
    def test_cfg_markers_inside_production_literals_and_comments_are_not_test_code(self):
        samples = [
            'fn text() -> &\'static str { r#"\n#[cfg(test)]\nmod tests {}\n"# }\nfn live() {}\n',
            '/*\n#[cfg(test)]\nmod external;\n*/\nfn live() {}\n',
        ]
        for source in samples:
            with self.subTest(source=source), tempfile.TemporaryDirectory() as temporary:
                workspace = Path(temporary)
                core = workspace / 'crates/bin-packing/src'
                core.mkdir(parents=True)
                path = core / 'lib.rs'
                path.write_text(source)
                ranges, external = REPORT.test_scope(core)
                self.assertEqual(ranges[path.resolve()], [])
                self.assertEqual(external, set())
                count = len(source.splitlines())
                lcov = f'SF:{path}\n' + ''.join(f'DA:{line},{int(line == count)}\n' for line in range(1, count + 1))
                lcov += f'BRDA:{count},0,0,1\nBRDA:{count},0,1,0\nend_of_record\n'
                totals = REPORT.summarize_lcov(lcov, workspace)['production_core']
                self.assertEqual((totals['lines_hit'], totals['lines']), (1, count))

    def test_test_scope_handles_literals_comments_lifetimes_and_external_modules(self):
        with tempfile.TemporaryDirectory() as temporary:
            core = Path(temporary)
            source = '''fn production() {}
#[cfg(test)]
mod tests {
    fn check<'a>(s: &'a str) {
        let raw = r###"}
}"###;
        let normal = "}\\\"{";
        let character = '}';
        /* } /* nested { */ } */
        // }
        assert_eq!(s, raw);
    }
}
fn still_production() {}
#[cfg(test)]
mod external;
'''
            path = core / 'model.rs'
            path.write_text(source)
            ranges, external = REPORT.test_scope(core)
            self.assertEqual(ranges[path.resolve()], [(2, 13), (15, 16)])
            self.assertEqual(external, {core / 'model/external', core / 'model/external.rs'})

    def test_unrecognized_or_unterminated_test_items_fail_closed(self):
        for source in ['#[cfg(test)]\nimpl Thing {}\n', '#[cfg(test)]\nmod tests {\n',
                       'mod nested {\n    #[cfg(test)]\n    fn test() {}\n}\n']:
            with self.subTest(source=source), tempfile.TemporaryDirectory() as temporary:
                core = Path(temporary)
                (core / 'lib.rs').write_text(source)
                with self.assertRaises(ValueError):
                    REPORT.test_scope(core)

    def test_production_metrics_exclude_inline_and_external_tests_without_losing_live_code(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary)
            core = workspace / 'crates/bin-packing/src'
            core.mkdir(parents=True)
            path = core / 'lib.rs'
            path.write_text('fn live() {}\n#[cfg(test)]\nmod tests { fn check() {} }\nfn missed() {}\n#[cfg(test)]\nmod external;\n')
            (core / 'external.rs').write_text('fn test() {}\n')
            lcov = (f'SF:{path}\nDA:1,5\nDA:3,100\nDA:4,0\nBRDA:1,0,0,3\nBRDA:1,0,1,-\n'
                    f'BRDA:3,0,0,100\nend_of_record\nSF:{core / "external.rs"}\nDA:1,100\nend_of_record\n')
            result = REPORT.summarize_lcov(lcov, workspace)
            self.assertEqual(len(result['files']), 1)
            self.assertEqual(result['production_core'], {'lines':2, 'lines_hit':1, 'branches':2, 'branches_hit':1,
                                                         'line_percent':50, 'branch_percent':50})
            self.assertEqual(result['files'][0]['missing_lines'], [4])

    def test_empty_or_duplicate_data_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary)
            core = workspace / 'crates/bin-packing/src'
            core.mkdir(parents=True)
            path = core / 'lib.rs'
            path.write_text('fn live() {}\n')
            for body in ['', f'SF:{path}\nDA:1,0\nDA:1,1\nend_of_record',
                         f'SF:{path}\nDA:1,-1\nBRDA:1,0,0,0\nend_of_record']:
                with self.subTest(body=body), self.assertRaises(ValueError):
                    REPORT.summarize_lcov(body, workspace)

    def test_cargo_json_paths_support_new_layouts_and_ignore_build_scripts(self):
        def artifact(path, kind):
            return json.dumps({'reason':'compiler-artifact', 'executable':path, 'target':{'kind':[kind]}})
        log = '\n'.join(['test hello ... ok', '{bad json', artifact('/new/build/crate/hash/out/test', 'lib'),
                         artifact('/new/build/crate/hash/out/test', 'lib'), artifact('/old/debug/deps/test', 'test'),
                         artifact('/new/build/build-script', 'custom-build')])
        self.assertEqual(REPORT.executable_artifacts(log), ['/new/build/crate/hash/out/test', '/old/debug/deps/test'])
        with self.assertRaises(ValueError):
            REPORT.executable_artifacts('test result: ok. 0 passed')


if __name__ == '__main__':
    unittest.main()
