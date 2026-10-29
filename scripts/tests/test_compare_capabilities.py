"""Independent objective, tree and nested-loading certificate validation."""

import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('capability_comparison', Path(__file__).resolve().parents[1] / 'compare-capabilities-packingsolver.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class CapabilityCertificateTests(unittest.TestCase):
    def case(self, name):
        return next(case for case in MODULE.fixtures() if case['name'] == name)

    def test_fixed_fixtures_and_axis_translation_are_reproducible(self):
        cases = MODULE.fixtures()
        self.assertEqual(len(cases), 14)
        self.assertEqual(cases, MODULE.fixtures())
        self.assertEqual(len({case['name'] for case in cases}), len(cases))
        translated = MODULE.upstream_input(self.case('loading-nesting'))
        self.assertEqual([translated['bin_types'][0][axis] for axis in ('x', 'y', 'z')], [4, 4, 6])
        self.assertEqual([translated['item_types'][0][axis] for axis in ('x', 'y', 'z')], [2, 2, 4])
        self.assertEqual(translated['item_types'][0]['maximum_weight_above'], 8)
        self.assertEqual(translated['bin_types'][0]['maximum_weight'], 20)
        self.assertEqual(translated['item_types'][0]['nesting_height'], 2)

    def test_knapsack_checks_physical_overlap_and_exact_source_values(self):
        case = self.case('1d-knapsack')
        layouts = [{'stock': 0, 'copies': 1, 'placements': [
            {'item': 1, 'position': [0], 'dimensions': [5]},
            {'item': 1, 'position': [5], 'dimensions': [5]},
        ]}]
        checked = MODULE.validate_geometry(case, layouts)
        self.assertEqual(checked['counts'], [0, 2])
        self.assertEqual(checked['profit'], 20)
        self.assertFalse(checked['complete'])
        layouts[0]['placements'][1]['position'][0] = 4
        with self.assertRaises(ValueError):
            MODULE.validate_geometry(case, layouts)

    def test_nesting_requires_exact_authorized_height_and_weight_above(self):
        case = self.case('loading-nesting')
        placements = []
        for stack, (x, z, count) in enumerate([(0, 0, 2), (2, 0, 2), (0, 2, 1)]):
            for copy_index in range(count):
                placements.append({'item': 0, 'position': [x, copy_index * 2, z],
                                   'dimensions': [2, 4, 2], 'stack': stack})
        layouts = [{'stock': 0, 'copies': 1, 'placements': placements}]
        self.assertEqual(MODULE.validate_geometry(case, layouts)['counts'], [5])
        corrupted = copy.deepcopy(layouts)
        corrupted[0]['placements'][1]['position'][1] = 1
        with self.assertRaises(ValueError):
            MODULE.validate_geometry(case, corrupted)
        case['problem']['item_rules'][0]['maximum_weight_above'] = 3
        with self.assertRaises(ValueError):
            MODULE.validate_geometry(case, layouts)

    def test_physical_partition_and_axis_levels_are_independently_checked(self):
        case = self.case('staged-3-axis-levels')
        regions = [(0, 0, 10, 10), (0, 0, 4, 10), (4, 0, 6, 10),
                   (4, 0, 6, 6), (4, 6, 6, 4), (4, 6, 3, 4), (7, 6, 3, 4)]
        children = [[1, 2], [], [3, 4], [], [5, 6], [], []]
        items = [None, 0, None, 1, None, 2, 2]
        rows = []
        for index, ((x, y, w, h), item) in enumerate(zip(regions, items)):
            parent = next((i for i, c in enumerate(children) if index in c), None)
            rows.append(dict(PLATE_ID='0', COPIES='1', NODE_ID=str(index), X=str(x), Y=str(y), WIDTH=str(w), HEIGHT=str(h), TYPE=str(-2 if item is None else item), CUT='0', PARENT='' if parent is None else str(parent)))
        MODULE.validate_staged(case, rows=rows)
        with self.assertRaises(ValueError):
            MODULE.validate_staged(self.case('staged-2-axis-levels'), rows=rows)
        rows[6]['X'] = '8'
        with self.assertRaises(ValueError):
            MODULE.validate_staged(case, rows=rows)


if __name__ == '__main__':
    unittest.main()
