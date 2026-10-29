"""Independent certificate checks, including repetition and malformed geometry."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / 'compare-packingsolver.py'
SPEC = importlib.util.spec_from_file_location('compare_packingsolver', SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CertificateTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix='packing-certificate-')
        self.addCleanup(self.folder.cleanup)
        self.case = {
            'dimension': 'two_d',
            'problem': {
                'sheets': [{'name': 'stock', 'width': 2, 'height': 1, 'cost': 1,
                            'quantity': None, 'kerf': 0, 'edge_kerf_relief': False}],
                'demands': [{'name': 'part', 'width': 1, 'height': 1,
                             'quantity': 2, 'can_rotate': False}],
            },
        }
        self.header = 'TYPE,ID,COPIES,BIN,X,Y,LX,LY,GROUP_ID\n'
        self.rows = ['BIN,0,1,0,0,0,2,1,', 'ITEM,0,1,0,0,0,1,1,0',
                     'ITEM,0,1,0,1,0,1,1,0']
        self.payload = {'Output': {'Time': 0.001, 'Solution': {
            'NumberOfItems': 2, 'NumberOfBins': 1, 'BinArea': 2,
            'ItemArea': 2, 'FullWaste': 0, 'BinCost': 1,
        }}}

    def validate(self, rows=None, header=None):
        path = Path(self.folder.name) / 'certificate.csv'
        path.write_text((header or self.header) + '\n'.join(rows or self.rows) + '\n')
        return MODULE.validate_certificate(self.case, path, self.payload)

    def test_valid_certificate(self):
        result = self.validate()
        self.assertEqual(result['bin_count'], 1)
        self.assertEqual(result['placed_count'], 2)

    def test_repeated_layout_copies_are_multiplied(self):
        self.case['problem']['demands'][0]['quantity'] = 4
        summary = self.payload['Output']['Solution']
        summary.update(NumberOfItems=4, NumberOfBins=2, BinArea=4, ItemArea=4, BinCost=2)
        result = self.validate([row.replace(',1,0,', ',2,0,', 1) for row in self.rows])
        self.assertEqual(result['bin_count'], 2)
        self.assertEqual(result['placed_count'], 4)

    def test_duplicate_bin_records_are_rejected(self):
        with self.assertRaises(ValueError):
            self.validate([self.rows[0]] + self.rows)

    def test_overlapping_duplicate_placement_is_rejected(self):
        with self.assertRaises(ValueError):
            self.validate([self.rows[0], self.rows[1], self.rows[1]])

    def test_dropped_demand_is_rejected(self):
        with self.assertRaises(ValueError):
            self.validate(self.rows[:-1])

    def test_wrong_summary_is_rejected(self):
        self.payload['Output']['Solution']['NumberOfBins'] = 99
        with self.assertRaises(ValueError):
            self.validate()

    def test_copy_count_disagreement_is_rejected(self):
        with self.assertRaises(ValueError):
            self.validate([self.rows[0], 'ITEM,0,2,0,0,0,1,1,0', self.rows[2]])

    def test_out_of_bounds_coordinates_are_rejected(self):
        with self.assertRaises(ValueError):
            self.validate([self.rows[0], self.rows[1], 'ITEM,0,1,0,2,0,1,1,0'])

    def test_missing_certificate_fields_are_rejected(self):
        with self.assertRaises(ValueError):
            self.validate(header=self.header.replace('GROUP_ID', 'MISSING'))

    def test_missing_summary_is_rejected(self):
        self.payload.clear()
        with self.assertRaises(ValueError):
            self.validate()

    def test_bounded_empty_incumbent_is_partial_and_never_a_zero_bin_optimum(self):
        path = Path(self.folder.name) / 'empty.csv'
        path.write_text(self.header)
        payload = {'Output': {'Time': 2.0, 'Solution': {
            'NumberOfItems': 0, 'NumberOfBins': 0, 'NumberOfUnpackedItems': 2,
            'BinArea': 0, 'ItemArea': 0, 'FullWaste': 0, 'BinCost': 0,
        }}}
        with self.assertRaises(ValueError):
            MODULE.validate_certificate(self.case, path, payload)
        checked = MODULE.validate_certificate(self.case, path, payload, allow_partial=True)
        self.assertFalse(checked['complete'])
        self.assertEqual(checked['placed_count'], 0)
        self.assertEqual(checked['requested_count'], 2)
        self.assertEqual(checked['unplaced_quantities'], [2])
        payload['Output']['Solution']['NumberOfUnpackedItems'] = 0
        with self.assertRaises(ValueError):
            MODULE.validate_certificate(self.case, path, payload, allow_partial=True)

    def test_three_dimensional_certificate_checks_rotation_mask(self):
        self.case = MODULE.case_3d('test', 'test', (3, 3, 2), [(2, 1, 1, 1, 1)])
        self.header = 'TYPE,ID,COPIES,BIN,X,Y,Z,LX,LY,LZ,ROTATION\n'
        self.rows = ['BIN,0,1,0,0,0,0,3,3,2,', 'ITEM,0,1,0,0,0,0,2,1,1,XYZ']
        self.payload = {'Output': {'Time': 0.001, 'Solution': {
            'NumberOfItems': 1, 'NumberOfBins': 1, 'BinVolume': 18,
            'ItemVolume': 2, 'FullWaste': 16, 'BinCost': 1,
        }}}
        self.assertEqual(self.validate()['bin_count'], 1)
        self.rows[1] = 'ITEM,0,1,0,0,0,0,1,2,1,YXZ'
        with self.assertRaises(ValueError):
            self.validate()

    def test_rust_results_are_validated_independently(self):
        result = {'unplaced': [], 'sheet_count': 1, 'total_cost': 1,
                  'total_waste_area': 0, 'total_kerf_area': 0, 'layouts': [{
                      'sheet_name': 'stock', 'width': 2, 'height': 1, 'cost': 1,
                      'used_area': 2, 'waste_area': 0, 'kerf_area': 0, 'placements': [
                          {'name': 'part', 'x': 0, 'y': 0, 'width': 1, 'height': 1,
                           'rotated': False},
                          {'name': 'part', 'x': 1, 'y': 0, 'width': 1, 'height': 1,
                           'rotated': False},
                      ]}]}
        self.assertEqual(MODULE.validate_rust_result(self.case, result)['placed_count'], 2)
        result['layouts'][0]['placements'][1]['x'] = 0
        with self.assertRaises(ValueError):
            MODULE.validate_rust_result(self.case, result)

    def test_generated_inputs_are_reproducible(self):
        first = MODULE.generated_cases()
        self.assertEqual(len(first), 22)
        self.assertEqual(first, MODULE.generated_cases())
        self.assertEqual(len({case['name'] for case in first}), 22)
        categories = {case['name']: case['category'] for case in first}
        self.assertEqual(categories['2d-elongated'], 'targeted_regression')
        self.assertEqual(categories['3d-seed-29'], 'targeted_regression')
        self.assertEqual(categories['2d-seed-421'], 'targeted_regression')
        for name in ('2d-seed-1301', '2d-seed-1699', '3d-seed-1201', '3d-seed-1613'):
            self.assertEqual(categories[name], 'synthetic_held_out')

    def test_public_geometry_preserves_copies_and_rotation_mask(self):
        root = Path(self.folder.name)
        for collection, names in [
            ('rectangle/beasley1985', ['gcut1.txt', 'gcut2.txt']),
            ('rectangle/christofides1977', ['cgcut1.txt', 'cgcut2.txt']),
            ('box/loh1992', ['thpack8.txt_1', 'thpack8.txt_2']),
            ('box/ivancic1989', ['thpack9.txt_1', 'thpack9.txt_2']),
        ]:
            folder = root / collection
            folder.mkdir(parents=True)
            for name in names:
                if collection.startswith('rectangle'):
                    bins = 'ID,WIDTH,HEIGHT\n0,4,5\n'
                    items = 'ID,WIDTH,HEIGHT,COPIES,PROFIT\n0,2,3,4,19\n'
                else:
                    bins = 'ID,X,Y,Z\n0,4,5,6\n'
                    items = ('ID,X,Y,Z,COPIES,ROTATION_XYZ,ROTATION_XZY,ROTATION_YXZ,'
                             'ROTATION_YZX,ROTATION_ZXY,ROTATION_ZYX\n0,2,3,4,7,1,0,1,0,0,0\n')
                (folder / (name + '_bins.csv')).write_text(bins)
                (folder / (name + '_items.csv')).write_text(items)
        cases = MODULE.public_cases(root)
        self.assertEqual(len(cases), 8)
        self.assertEqual(cases, MODULE.public_cases(root))
        self.assertEqual(cases[0]['problem']['demands'][0]['quantity'], 4)
        self.assertFalse(cases[0]['problem']['demands'][0]['can_rotate'])
        self.assertEqual(cases[4]['problem']['demands'][0]['allowed_rotations'], 5)
        self.assertEqual(cases[4]['problem']['demands'][0]['quantity'], 7)
        self.assertEqual(cases[0]['problem']['sheets'][0]['cost'], 1)
        self.assertEqual(cases[4]['public_source']['revision'], MODULE.UPSTREAM_REVISION)
        broken = root / 'box/loh1992/thpack8.txt_1_items.csv'
        broken.write_text(broken.read_text().replace(',7,1,0,1,0,0,0', ',7,2,0,1,0,0,0'))
        with self.assertRaises(ValueError):
            MODULE.public_cases(root)


if __name__ == '__main__':
    unittest.main()
