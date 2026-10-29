"""Independent irregular adapter, original-geometry and corruption regressions."""

import copy
import importlib.util
import math
from pathlib import Path
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / 'compare-irregular-packingsolver.py'
SPEC = importlib.util.spec_from_file_location('compare_irregular', SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def rectangle(width, height, x=0, y=0):
    return {'type': 'rectangle', 'width': width, 'height': height, 'x': x, 'y': y}


def polygon(points):
    return {'type': 'polygon', 'vertices': [{'x': x, 'y': y} for x, y in points]}


def placement(item, x=0, y=0, angle=0, mirrored=False):
    return {'demand_index': item, 'x': x, 'y': y,
            'rotation_degrees': angle, 'mirrored': mirrored}


def layout(placements, copies=1, stock=0):
    return {'bin_index': stock, 'copies': copies, 'placements': placements}


class IrregularGeometryTests(unittest.TestCase):
    def test_actual_material_interlocking_and_hole_occupancy(self):
        source = {'objective': 'feasibility', 'bin_types': [rectangle(2, 3)],
                  'item_types': [dict(polygon([(0, 0), (2, 0), (2, 1), (1, 1),
                                               (1, 2), (0, 2)]), copies=2,
                                      allowed_rotations=[{'start': 0, 'end': 0},
                                                         {'start': 180, 'end': 180}])]}
        case = MODULE.normalize_upstream(source)
        checked = MODULE.validate_layouts(case, [layout([
            placement(0), placement(0, 2, 3, 180)])])
        self.assertTrue(checked['feasible'])
        self.assertEqual(checked['used_area'], 6)

        frame = dict(rectangle(4, 4), holes=[rectangle(2, 2, 1, 1)])
        case = MODULE.normalize_upstream({'objective': 'feasibility',
            'bin_types': [rectangle(4, 4)], 'item_types': [frame, rectangle(2, 2)]})
        self.assertTrue(MODULE.validate_layouts(case, [layout([
            placement(0), placement(1, 1, 1)])])['feasible'])
        with self.assertRaisesRegex(ValueError, 'overlap'):
            MODULE.validate_layouts(case, [layout([placement(0), placement(1)])])

    def test_explicit_defaults_and_roundtrip_preserve_geometry_and_permissions(self):
        source = {'objective': 'knapsack', 'bin_types': [rectangle(5, 4)],
                  'item_types': [dict(rectangle(2, 3), allow_mirroring=True)]}
        case = MODULE.normalize_upstream(source)
        self.assertEqual(case['problem']['bins'][0]['cost'], 20)
        self.assertEqual(case['problem']['bins'][0]['quantity'], 1)
        self.assertEqual(case['problem']['demands'][0]['profit'], 6)
        self.assertTrue(case['problem']['demands'][0]['can_mirror'])
        rebuilt = MODULE.normalize_upstream(MODULE.upstream_input(case))
        self.assertEqual(case['problem'], rebuilt['problem'])
        self.assertEqual(case['options'], rebuilt['options'])

    def test_wrong_transforms_counts_clearance_and_inventory_fail_closed(self):
        source = {'objective': 'knapsack', 'parameters': {'item_item_minimum_spacing': 1},
                  'bin_types': [rectangle(5, 2)],
                  'item_types': [dict(rectangle(2, 2), copies=2)]}
        case = MODULE.normalize_upstream(source)
        valid = [layout([placement(0), placement(0, 3)])]
        self.assertTrue(MODULE.validate_layouts(case, valid)['feasible'])
        corruptions = []
        for key, value in [('x', 2), ('demand_index', 1), ('mirrored', True),
                           ('rotation_degrees', 180), ('y', float('nan'))]:
            bad = copy.deepcopy(valid)
            bad[0]['placements'][1][key] = value
            corruptions.append(bad)
        corruptions.extend([[layout([placement(0)], copies=2)], valid + valid])
        for bad in corruptions:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                MODULE.validate_layouts(case, bad)

    def test_circles_arcs_and_distances_use_original_curves(self):
        case = MODULE.normalize_upstream({'objective': 'feasibility',
            'bin_types': [rectangle(4, 2)],
            'item_types': [{'type': 'circle', 'radius': 1, 'copies': 2}]})
        result = MODULE.validate_layouts(case, [layout([placement(0, 1, 1), placement(0, 3, 1)])])
        self.assertAlmostEqual(result['used_area'], 2 * math.pi)
        with self.assertRaisesRegex(ValueError, 'overlap'):
            MODULE.validate_layouts(case, [layout([placement(0, 1, 1), placement(0, 2.9, 1)])])
        upper_half = {'type': 'general', 'elements': [
            {'type': 'LineSegment', 'start': {'x': 0, 'y': 0}, 'end': {'x': 2, 'y': 0}},
            {'type': 'CircularArc', 'start': {'x': 2, 'y': 0}, 'end': {'x': 0, 'y': 0},
             'center': {'x': 1, 'y': 0}, 'orientation': 'Anticlockwise'}]}
        case = MODULE.normalize_upstream({'objective': 'feasibility',
            'bin_types': [rectangle(2, 0.9)], 'item_types': [upper_half]})
        with self.assertRaisesRegex(ValueError, 'outside'):
            MODULE.validate_layouts(case, [layout([placement(0)])])
        geometry = MODULE.shape_geometry(case['problem']['demands'][0]['shape'])
        self.assertAlmostEqual(MODULE.area(geometry), math.pi / 2)
        circle = MODULE.arc((1, 0), (1, 0), (0, 0))
        other = MODULE.arc((4, 0), (4, 0), (3, 0))
        line = MODULE.Edge((-2, 2), (2, 2), None, 0)
        self.assertAlmostEqual(MODULE.primitive_distance(circle, other), 1)
        self.assertAlmostEqual(MODULE.primitive_distance(circle, line), 1)

    def test_mirroring_interval_angles_components_and_defects(self):
        source = {'objective': 'feasibility', 'bin_types': [rectangle(3, 2)],
                  'item_types': [{'shapes': [rectangle(1, 2), rectangle(1, 2, 2, 0)]}, rectangle(1, 2)]}
        case = MODULE.normalize_upstream(source)
        self.assertTrue(MODULE.validate_layouts(case, [layout([placement(0), placement(1, 1)])])['feasible'])
        interval = MODULE.normalize_upstream({'objective': 'feasibility', 'bin_types': [rectangle(3, 1)],
            'item_types': [dict(rectangle(1, 3), allowed_rotations=[{'start': 89, 'end': 91}])]})
        self.assertTrue(MODULE.validate_layouts(interval, [layout([placement(0, 3, 0, 90)])])['feasible'])
        with self.assertRaisesRegex(ValueError, 'rotation'):
            MODULE.validate_layouts(interval, [layout([placement(0, 3, 0, 180)])])
        defect = MODULE.normalize_upstream({'objective': 'knapsack',
            'bin_types': [dict(rectangle(5, 2), defects=[dict(rectangle(1, 2, 2, 0),
                                                           item_defect_minimum_spacing=0.25)])],
            'item_types': [rectangle(2, 2)]})
        with self.assertRaisesRegex(ValueError, 'defect clearance'):
            MODULE.validate_layouts(defect, [layout([placement(0)])])
        with self.assertRaisesRegex(ValueError, 'defect material overlap'):
            MODULE.validate_layouts(defect, [layout([placement(0, 1)])])

    def test_every_fixed_targeted_case_has_an_exact_translation_roundtrip(self):
        cases = MODULE.targeted_cases()
        self.assertEqual(len(cases), 14)
        for case in cases:
            with self.subTest(case=case['name']):
                rebuilt = MODULE.normalize_upstream(MODULE.upstream_input(case))
                self.assertEqual(case['problem'], rebuilt['problem'])
                self.assertEqual(case['options'], rebuilt['options'])

    def test_unmatched_objectives_and_permissions_are_rejected(self):
        for source in [
            {'objective': 'open-dimension-xy', 'parameters': {'open_dimension_xy_aspect_ratio': 1},
             'bin_types': [rectangle(2, 2)], 'item_types': [rectangle(1, 1)]},
            {'objective': 'feasibility', 'bin_types': [rectangle(2, 2)],
             'item_types': [dict(rectangle(1, 1), allowed_rotations=[{'start': 0, 'end': 0, 'mirror': True}])]},
        ]:
            with self.assertRaises(ValueError):
                MODULE.normalize_upstream(source)


def certificate_rectangle(width, height, x=0, y=0):
    points = [(x, y), (x + width, y), (x + width, y + height), (x, y + height)]
    return [{'type': 'LineSegment', 'xs': a[0], 'ys': a[1], 'xe': b[0], 'ye': b[1]}
            for a, b in zip(points, points[1:] + points[:1])]


class IrregularCertificateTests(unittest.TestCase):
    def setUp(self):
        self.case = MODULE.normalize_upstream({'objective': 'bin-packing',
            'bin_types': [rectangle(2, 1)], 'item_types': [dict(rectangle(1, 1), copies=2)]})
        self.certificate = {'bins': [{'id': 0, 'copies': 1,
            'shape': certificate_rectangle(2, 1), 'items': [
                {'id': 0, 'x': x, 'y': 0, 'angle': 0, 'mirror': False,
                 'item_shapes': [{'shape': certificate_rectangle(1, 1, x)}]} for x in (0, 1)]}]}
        self.payload = {'Output': {'Time': 0.01, 'Solution': {
            'NumberOfItems': 2, 'NumberOfBins': 1, 'ItemArea': 2, 'ItemProfit': 2,
            'BinArea': 2, 'BinCost': 2, 'FullWaste': 0, 'XMin': 0, 'YMin': 0, 'XMax': 2, 'YMax': 1}}}

    def test_rich_certificate_source_shapes_and_all_totals_are_checked(self):
        checked = MODULE.validate_certificate(self.case, self.certificate, self.payload)
        self.assertTrue(checked['feasible'])
        for key in self.payload['Output']['Solution']:
            bad = copy.deepcopy(self.payload)
            bad['Output']['Solution'][key] += 1
            with self.subTest(key=key), self.assertRaises(ValueError):
                MODULE.validate_certificate(self.case, self.certificate, bad)
        corruptions = []
        bad = copy.deepcopy(self.certificate)
        bad['bins'][0]['items'][0]['item_shapes'][0]['shape'][0]['xe'] = 0.9
        corruptions.append(bad)
        bad = copy.deepcopy(self.certificate)
        bad['bins'][0]['items'][0]['item_shapes'][0]['holes'] = [certificate_rectangle(0.5, 0.5)]
        corruptions.append(bad)
        bad = copy.deepcopy(self.certificate)
        bad['bins'][0]['shape'][0]['xe'] = 3
        corruptions.append(bad)
        bad = copy.deepcopy(self.certificate)
        bad['bins'][0]['copies'] = 2
        corruptions.append(bad)
        bad = copy.deepcopy(self.certificate)
        ring = bad['bins'][0]['items'][0]['item_shapes'][0]['shape']
        ring[1], ring[2] = ring[2], ring[1]
        corruptions.append(bad)
        for bad in corruptions:
            with self.assertRaises(ValueError):
                MODULE.validate_certificate(self.case, bad, self.payload)

    def test_native_totals_geometry_shortfalls_and_probe_limits_are_checked(self):
        options = dict(self.case['options'], max_states=4096, curve_tolerance=0.01)
        result = {'algorithm': 'native_irregular_contact', 'options': options,
                  'layouts': [{'bin_index': 0, 'placements': [placement(0), placement(0, 1)],
                               'used_area': 2, 'used_width': 2, 'used_height': 1}],
                  'unplaced_quantities': [0], 'feasible': True, 'total_cost': 2, 'total_profit': 2,
                  'search_truncated': True, 'explored_states': 10, 'construction_probes': 12}
        self.assertTrue(MODULE.validate_rust(self.case, result, options)['feasible'])
        for key, value in [('total_cost', 3), ('total_profit', 3), ('unplaced_quantities', [1]),
                           ('feasible', False), ('unplaced_quantities', [False]),
                           ('explored_states', 4097), ('construction_probes', 32769)]:
            bad = copy.deepcopy(result)
            bad[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                MODULE.validate_rust(self.case, bad, options)

    def test_empty_reference_solution_conserves_every_unplaced_source(self):
        payload = {'Output': {'Time': 0.01, 'Solution': {
            'NumberOfItems': 0, 'NumberOfBins': 0, 'ItemArea': 0, 'ItemProfit': 0,
            'BinArea': 0, 'BinCost': 0, 'FullWaste': 0,
            'XMin': None, 'YMin': None, 'XMax': None, 'YMax': None}}}
        checked = MODULE.validate_certificate(self.case, None, payload)
        self.assertFalse(checked['feasible'])
        self.assertEqual(checked['unplaced_quantities'], [2])

    def test_reference_extrema_describe_the_last_bin_group(self):
        case = copy.deepcopy(self.case)
        case['problem']['bins'][0]['quantity'] = 2
        case['problem']['demands'][0]['quantity'] = 3
        certificate = copy.deepcopy(self.certificate)
        certificate['bins'].append({'id': 0, 'copies': 1, 'shape': certificate_rectangle(2, 1),
            'items': [{'id': 0, 'x': 0.5, 'y': 0, 'angle': 0, 'mirror': False,
                       'item_shapes': [{'shape': certificate_rectangle(1, 1, 0.5)}]}]})
        payload = {'Output': {'Time': 0.01, 'Solution': {
            'NumberOfItems': 3, 'NumberOfBins': 2, 'ItemArea': 3, 'ItemProfit': 3,
            'BinArea': 4, 'BinCost': 4, 'FullWaste': 1,
            'XMin': 0.5, 'YMin': 0, 'XMax': 1.5, 'YMax': 1}}}
        self.assertTrue(MODULE.validate_certificate(case, certificate, payload)['feasible'])
        payload['Output']['Solution']['XMax'] = 2
        with self.assertRaisesRegex(ValueError, 'XMax'):
            MODULE.validate_certificate(case, certificate, payload)


if __name__ == '__main__':
    unittest.main()
