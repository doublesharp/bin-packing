#!/usr/bin/env python3
"""Compare validated native release results against a pinned PackingSolver build."""

import argparse
import csv
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import statistics
import subprocess
import sys
import time


UPSTREAM_REVISION = '376b8d1cc4aed5b171bfe1219c037c661faaa7cf'
ROTATIONS = {'xyz': (0, 1, 2), 'xzy': (0, 2, 1), 'yxz': (1, 0, 2),
             'yzx': (1, 2, 0), 'zxy': (2, 0, 1), 'zyx': (2, 1, 0)}
ROOT = Path(__file__).resolve().parents[1]


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def integer(value, label, minimum=0):
    require(type(value) is int and value >= minimum, 'invalid integer: ' + label)
    return value


def csv_integer(row, key, minimum=0):
    value = row.get(key)
    require(isinstance(value, str) and re.fullmatch(r'-?\d+', value), 'invalid CSV ' + key)
    return integer(int(value), key, minimum)


def dimensions(value, axes):
    return tuple(integer(value.get(axis), axis, 1) for axis in axes)


def numeric(value, label):
    require(type(value) in (int, float) and math.isfinite(value), 'invalid numeric ' + label)
    return value


def equal_number(actual, expected, label):
    require(math.isclose(numeric(actual, label), expected, rel_tol=1e-9, abs_tol=1e-9),
            'mismatched total ' + label)


def source(case):
    dimension = case['dimension']
    require(dimension in ('two_d', 'three_d'), 'unsupported comparison dimension')
    axes = ('width', 'height') if dimension == 'two_d' else ('width', 'height', 'depth')
    stocks = case['problem']['sheets' if dimension == 'two_d' else 'bins']
    require(len(stocks) == 1, 'comparison requires one stock type')
    stock = stocks[0]
    if dimension == 'two_d':
        require(stock.get('kerf', 0) == 0 and not stock.get('edge_kerf_relief', False),
                'comparison requires zero kerf and no edge relief')
    return axes, stock, case['problem']['demands']


def validate_layouts(case, layouts, allow_partial=False):
    """Independent integer geometry and exact-demand validation for either engine."""
    axes, stock, demands = source(case)
    stock_dimensions = dimensions(stock, axes)
    quantity = sum(integer(d.get('quantity'), 'demand.quantity', 1) for d in demands)
    require(len(layouts) <= quantity and (layouts or allow_partial), 'missing or excessive layouts')
    counts = [0] * len(demands)
    bin_ids = set()
    bin_count, used = 0, 0
    for layout in layouts:
        bin_id = integer(layout.get('id'), 'bin.id')
        require(bin_id not in bin_ids, 'duplicate bin identifier')
        bin_ids.add(bin_id)
        copies = integer(layout.get('copies'), 'bin.copies', 1)
        require(copies <= quantity, 'excessive bin copies')
        require(tuple(layout.get('dimensions', ())) == stock_dimensions, 'wrong stock dimensions')
        placements = layout.get('placements')
        require(isinstance(placements, list) and placements, 'missing bin placements')
        for index, placement in enumerate(placements):
            item = integer(placement.get('item'), 'item.id')
            require(item < len(demands), 'unknown item identifier')
            demand = demands[item]
            expected = dimensions(demand, axes)
            position = placement.get('position')
            size = placement.get('dimensions')
            require(isinstance(position, (tuple, list)) and len(position) == len(axes),
                    'wrong coordinate count')
            require(isinstance(size, (tuple, list)) and len(size) == len(axes),
                    'wrong dimension count')
            position = tuple(integer(v, 'coordinate') for v in position)
            size = tuple(integer(v, 'item dimension', 1) for v in size)
            if len(axes) == 2:
                allowed = [expected]
                if demand['can_rotate']:
                    allowed.append(expected[::-1])
                require(size in allowed, 'invalid rectangle orientation or dimensions')
            else:
                rotation = placement.get('rotation')
                require(rotation in ROTATIONS, 'unknown 3D rotation')
                rotation_index = list(ROTATIONS).index(rotation)
                require(demand['allowed_rotations'] & (1 << rotation_index), 'disallowed rotation')
                require(size == tuple(expected[i] for i in ROTATIONS[rotation]),
                        'wrong dimensions for 3D rotation')
            require(all(p + s <= limit for p, s, limit in zip(position, size, stock_dimensions)),
                    'placement outside stock')
            for previous in placements[:index]:
                require(any(position[axis] + size[axis] <= previous['position'][axis]
                            or previous['position'][axis] + previous['dimensions'][axis]
                            <= position[axis] for axis in range(len(axes))),
                        'overlapping placements')
            counts[item] += copies
            used += math.prod(size) * copies
        bin_count += copies
    require(all(count <= demand['quantity'] for count, demand in zip(counts, demands)),
            'duplicated demand')
    complete = counts == [d['quantity'] for d in demands]
    require(complete or allow_partial, 'dropped demand')
    require(stock.get('quantity') is None or bin_count <= stock['quantity'], 'stock cap exceeded')
    return {'bin_count': bin_count, 'placed_count': sum(counts), 'requested_count': quantity,
            'complete': complete,
            'unplaced_quantities': [d['quantity'] - count for d, count in zip(demands, counts)],
            'used_measure': used,
            'waste_measure': bin_count * math.prod(stock_dimensions) - used,
            'cost': bin_count * stock['cost']}


def validate_certificate(case, path, payload, allow_partial=False):
    """Fail closed on malformed, partial, overlapping or inconsistent certificates."""
    axes, _, _ = source(case)
    is_three = len(axes) == 3
    expected_header = (['TYPE', 'ID', 'COPIES', 'BIN', 'X', 'Y', 'Z', 'LX', 'LY', 'LZ', 'ROTATION']
                       if is_three else ['TYPE', 'ID', 'COPIES', 'BIN', 'X', 'Y', 'LX', 'LY', 'GROUP_ID'])
    with Path(path).open(newline='') as stream:
        reader = csv.DictReader(stream)
        require(reader.fieldnames == expected_header, 'malformed certificate header')
        rows = list(reader)
    layouts = {}
    positions = ['X', 'Y', 'Z'][:len(axes)]
    extents = ['LX', 'LY', 'LZ'][:len(axes)]
    for row in rows:
        require(set(row) == set(expected_header) and all(v is not None for v in row.values()),
                'malformed certificate row')
        require(row['TYPE'] in ('BIN', 'ITEM'), 'unknown certificate row type')
        if row['TYPE'] == 'BIN':
            bin_id = csv_integer(row, 'BIN')
            require(bin_id not in layouts, 'duplicate bin identifier')
            require(csv_integer(row, 'ID') == 0, 'unknown stock identifier')
            require(all(csv_integer(row, coordinate) == 0 for coordinate in positions),
                    'nonzero stock origin')
            layouts[bin_id] = {'id': bin_id, 'copies': csv_integer(row, 'COPIES', 1),
                               'dimensions': tuple(csv_integer(row, key, 1) for key in extents),
                               'placements': []}
    for row in rows:
        if row['TYPE'] != 'ITEM':
            continue
        bin_id = csv_integer(row, 'BIN')
        require(bin_id in layouts, 'item references missing bin')
        layout = layouts[bin_id]
        require(csv_integer(row, 'COPIES', 1) == layout['copies'], 'inconsistent copy counts')
        placement = {'item': csv_integer(row, 'ID'),
                     'position': tuple(csv_integer(row, key) for key in positions),
                     'dimensions': tuple(csv_integer(row, key, 1) for key in extents)}
        if is_three:
            placement['rotation'] = row['ROTATION'].lower()
        else:
            require(csv_integer(row, 'GROUP_ID') == 0, 'unexpected item group')
        layout['placements'].append(placement)
    checked = validate_layouts(case, list(layouts.values()), allow_partial=allow_partial)
    require(isinstance(payload, dict) and isinstance(payload.get('Output'), dict),
            'missing upstream output summary')
    output = payload['Output']
    summary = output.get('Solution')
    require(isinstance(summary, dict), 'missing upstream solution summary')
    for key, expected in [('NumberOfItems', checked['placed_count']),
                          ('NumberOfBins', checked['bin_count']),
                          ('ItemVolume' if is_three else 'ItemArea', checked['used_measure']),
                          ('BinVolume' if is_three else 'BinArea',
                           checked['used_measure'] + checked['waste_measure']),
                          ('FullWaste', checked['waste_measure']), ('BinCost', checked['cost'])]:
        equal_number(summary.get(key), expected, key)
    if 'NumberOfUnpackedItems' in summary:
        equal_number(summary['NumberOfUnpackedItems'],
                     checked['requested_count'] - checked['placed_count'], 'NumberOfUnpackedItems')
    require(numeric(output.get('Time'), 'Time') >= 0, 'negative solve time')
    return checked


def validate_rust_result(case, result):
    """Validate Rust output without relying on its internal candidate validator."""
    axes, stock, demands = source(case)
    solution = result.get('solution', result)
    require(isinstance(solution, dict) and solution.get('unplaced') == [], 'partial Rust solution')
    names = {d['name']: index for index, d in enumerate(demands)}
    require(len(names) == len(demands), 'comparison demands require unique names')
    layouts = []
    raw_layouts = solution.get('layouts')
    require(isinstance(raw_layouts, list), 'missing Rust layouts')
    for index, raw in enumerate(raw_layouts):
        require(raw.get('sheet_name' if len(axes) == 2 else 'bin_name') == stock['name'],
                'unknown Rust stock label')
        equal_number(raw.get('cost'), stock['cost'], 'layout.cost')
        placements = []
        require(isinstance(raw.get('placements'), list), 'missing Rust placements')
        for placement in raw['placements']:
            require(placement.get('name') in names, 'unknown Rust demand label')
            item = names[placement['name']]
            normalized = {'item': item,
                          'position': tuple(placement.get(key) for key in ['x', 'y', 'z'][:len(axes)]),
                          'dimensions': tuple(placement.get(key) for key in axes)}
            if len(axes) == 3:
                normalized['rotation'] = placement.get('rotation')
            else:
                rotated = placement.get('rotated')
                require(type(rotated) is bool, 'missing rotation flag')
                require(not rotated or demands[item]['can_rotate'], 'disallowed Rust rotation')
                expected = dimensions(demands[item], axes)
                require(normalized['dimensions'] == (expected[::-1] if rotated else expected),
                        'dimensions disagree with rotation flag')
            placements.append(normalized)
        measure = sum(math.prod(p['dimensions']) for p in placements)
        equal_number(raw.get('used_area' if len(axes) == 2 else 'used_volume'), measure,
                     'layout.used_measure')
        equal_number(raw.get('waste_area' if len(axes) == 2 else 'waste_volume'),
                     math.prod(dimensions(stock, axes)) - measure, 'layout.waste_measure')
        if len(axes) == 2:
            equal_number(raw.get('kerf_area'), 0, 'layout.kerf_area')
        layouts.append({'id': index, 'copies': 1, 'dimensions': dimensions(raw, axes),
                        'placements': placements})
    checked = validate_layouts(case, layouts)
    equal_number(solution.get('sheet_count' if len(axes) == 2 else 'bin_count'),
                 checked['bin_count'], 'bin_count')
    equal_number(solution.get('total_waste_area' if len(axes) == 2 else 'total_waste_volume'),
                 checked['waste_measure'], 'total_waste')
    equal_number(solution.get('total_cost'), checked['cost'], 'total_cost')
    if len(axes) == 2:
        equal_number(solution.get('total_kerf_area'), 0, 'total_kerf_area')
    return checked


def case_2d(name, category, size, parts):
    return {'name': name, 'category': category, 'dimension': 'two_d', 'problem': {
        'sheets': [{'name': 'stock', 'width': size[0], 'height': size[1], 'cost': 1,
                    'quantity': None, 'kerf': 0, 'edge_kerf_relief': False}],
        'demands': [{'name': 'part_' + str(index), 'width': w, 'height': h,
                     'quantity': q, 'can_rotate': rotate}
                    for index, (w, h, q, rotate) in enumerate(parts)]}}


def case_3d(name, category, size, parts):
    return {'name': name, 'category': category, 'dimension': 'three_d', 'problem': {
        'bins': [{'name': 'stock', 'width': size[0], 'height': size[1], 'depth': size[2],
                  'cost': 1, 'quantity': None}],
        'demands': [{'name': 'part_' + str(index), 'width': w, 'height': h, 'depth': d,
                     'quantity': q, 'allowed_rotations': mask}
                    for index, (w, h, d, q, mask) in enumerate(parts)]}}


def generated_cases():
    cases = [
        case_2d('2d-homogeneous', 'targeted', (100, 80), [(20, 20, 40, True)]),
        case_2d('2d-geometric-bound', 'targeted', (10, 6),
                [(6, 4, 2, False), (4, 6, 2, False), (4, 3, 2, False)]),
        case_2d('2d-rotated-fit', 'targeted', (4, 9), [(9, 4, 3, True)]),
        case_2d('2d-elongated', 'targeted_regression', (60, 20),
                [(21, 6, 3, True), (17, 5, 4, False), (9, 8, 2, True), (33, 4, 2, False)]),
        case_2d('2d-repeated-mix', 'synthetic_held_out', (40, 30),
                [(9, 7, 10, True), (11, 5, 8, True), (6, 13, 4, True)]),
        case_3d('3d-homogeneous', 'targeted', (10, 10, 10), [(5, 5, 5, 16, 63)]),
        case_3d('3d-geometric-bound', 'targeted', (10, 10, 10), [(4, 4, 4, 17, 1)]),
        case_3d('3d-slabs', 'synthetic_held_out', (30, 15, 10),
                [(13, 7, 2, 6, 63), (14, 6, 4, 3, 63), (8, 5, 3, 5, 1)]),
    ]
    for seed in (17, 71, 193, 421, 947, 1301, 1699):
        rng = random.Random(seed)
        parts = [(rng.randint(2, 13), rng.randint(2, 10), rng.randint(1, 3),
                  bool(rng.getrandbits(1))) for _ in range(8)]
        category = 'targeted_regression' if seed == 421 else 'synthetic_held_out'
        cases.append(case_2d('2d-seed-' + str(seed), category, (24, 18), parts))
    for seed in (29, 103, 271, 431, 953, 1201, 1613):
        rng = random.Random(seed)
        parts = [(rng.randint(2, 8), rng.randint(2, 8), rng.randint(2, 8),
                  rng.randint(1, 2), 63 if rng.getrandbits(1) else 1) for _ in range(8)]
        category = 'targeted_regression' if seed == 29 else 'synthetic_held_out'
        cases.append(case_3d('3d-seed-' + str(seed), category, (16, 12, 10), parts))
    return cases


def public_cases(data_root):
    """Fixed public geometry fixtures, normalized to a shared bin-count problem.

    Selection is specified before comparing outputs. Preserve item sizes, copies
    and explicit rotation permissions; normalize price to one and inventory to
    unlimited. These are not scores for the collections' original objectives.
    """
    result = []
    selections = [
        ('rectangle/beasley1985', 'gcut1.txt', 'two_d'),
        ('rectangle/beasley1985', 'gcut2.txt', 'two_d'),
        ('rectangle/christofides1977', 'cgcut1.txt', 'two_d'),
        ('rectangle/christofides1977', 'cgcut2.txt', 'two_d'),
        ('box/loh1992', 'thpack8.txt_1', 'three_d'),
        ('box/loh1992', 'thpack8.txt_2', 'three_d'),
        ('box/ivancic1989', 'thpack9.txt_1', 'three_d'),
        ('box/ivancic1989', 'thpack9.txt_2', 'three_d'),
    ]
    for collection, instance, dimension in selections:
        paths = [data_root / collection / (instance + suffix)
                 for suffix in ('_bins.csv', '_items.csv')]
        parsed = []
        for path in paths:
            with path.open(newline='') as stream:
                parsed.append(list(csv.DictReader(stream)))
        bins, items = parsed
        require(len(bins) == 1, 'public fixture must have one stock')
        require(items, 'empty public fixture')
        quantity = lambda row: csv_integer(row, 'COPIES', 1) if 'COPIES' in row else 1
        name = 'public-' + collection.replace('/', '-') + '-' + instance
        if dimension == 'two_d':
            size = tuple(csv_integer(bins[0], field, 1) for field in ('WIDTH', 'HEIGHT'))
            parts = [(csv_integer(row, 'WIDTH', 1), csv_integer(row, 'HEIGHT', 1),
                      quantity(row), False) for row in items]
            case = case_2d(name, 'public_geometry', size, parts)
        else:
            size = tuple(csv_integer(bins[0], field, 1) for field in ('X', 'Y', 'Z'))
            parts = []
            for row in items:
                flags = [csv_integer(row, 'ROTATION_' + rotation.upper())
                         for rotation in ROTATIONS]
                require(all(flag in (0, 1) for flag in flags), 'invalid public rotation flag')
                mask = sum((1 << index) for index, flag in enumerate(flags) if flag)
                require(mask > 0, 'public item has no allowed rotation')
                parts.append((*[csv_integer(row, field, 1) for field in ('X', 'Y', 'Z')],
                              quantity(row), mask))
            case = case_3d(name, 'public_geometry', size, parts)
        case['public_source'] = {
            'revision': UPSTREAM_REVISION,
            'files': {str(path.relative_to(data_root)): file_hash(path) for path in paths},
            'normalization': 'original geometry/copies/explicit rotations; unlisted rectangle rotations forbidden; unit-price unlimited stock; bin-count objective',
        }
        result.append(case)
    return result


def upstream_input(case):
    axes, stock, demands = source(case)
    translated_axes = ('x', 'y', 'z')[:len(axes)]
    quantity = sum(d['quantity'] for d in demands)
    bin_type = dict(zip(translated_axes, dimensions(stock, axes)), copies=quantity, cost=stock['cost'])
    items = []
    for demand in demands:
        item = dict(zip(translated_axes, dimensions(demand, axes)), copies=demand['quantity'])
        if len(axes) == 2:
            item['oriented'] = not demand['can_rotate']
        else:
            item['rotations'] = [name.upper() for index, name in enumerate(ROTATIONS)
                                 if demand['allowed_rotations'] & (1 << index)]
        items.append(item)
    return {'objective': 'bin-packing', 'bin_types': [bin_type], 'item_types': items}


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def file_hash(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def command_output(command, cwd=None):
    return subprocess.run(command, cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


def source_metadata():
    paths = sorted((ROOT / 'crates/bin-packing/src').rglob('*.rs'))
    paths += [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', ROOT / 'crates/bin-packing/Cargo.toml',
              ROOT / 'crates/bin-packing/examples/packingsolver_compare.rs', Path(__file__)]
    hashes = {str(path.relative_to(ROOT)): file_hash(path) for path in paths}
    return {'git_revision': command_output(['git', 'rev-parse', 'HEAD'], ROOT),
            'git_status': command_output(['git', 'status', '--short'], ROOT),
            'source_files_sha256': hashes, 'combined_source_sha256': canonical_hash(hashes)}


def run_process(command, folder, environment, timeout, input_text=None):
    start = time.perf_counter()
    result = subprocess.run(command, input=input_text, env=environment, timeout=timeout,
                            capture_output=True, text=True)
    wall = time.perf_counter() - start
    (folder / 'stdout.log').write_text(result.stdout)
    (folder / 'stderr.log').write_text(result.stderr)
    result.check_returncode()
    return result, wall


def run_upstream(case, binary, folder, args, environment):
    translated = upstream_input(case)
    input_path = folder / 'input.json'
    input_path.write_text(json.dumps(translated, indent=2) + '\n')
    samples = []
    for index in range(args.warmups + args.repeats):
        sample = folder / ('warmup-' + str(index) if index < args.warmups
                           else 'sample-' + str(index - args.warmups))
        sample.mkdir()
        command = [str(binary), '--input', str(input_path), '--optimization-mode',
                   'not-anytime-sequential', '--time-limit', str(args.time_limit),
                   '--linear-programming-solver', args.lp_solver,
                   '--verbosity-level', '0', '--only-write-at-the-end', '--output',
                   str(sample / 'output.json'), '--certificate', str(sample / 'certificate.csv')]
        _, wall = run_process(command, sample, environment, args.time_limit + 30)
        payload = json.loads((sample / 'output.json').read_text())
        checked = validate_certificate(case, sample / 'certificate.csv', payload, allow_partial=True)
        if index >= args.warmups:
            samples.append({'process_seconds': wall, 'internal_seconds': payload['Output']['Time'],
                            'process_minus_internal_seconds': wall - payload['Output']['Time'],
                            'validated': checked, 'full_result': payload,
                            'certificate': (sample / 'certificate.csv').read_text(),
                            'command': command})
    counts = [sample['validated']['bin_count'] for sample in samples]
    return {'samples': samples, 'sample_bin_counts': counts,
            'sample_placed_counts': [sample['validated']['placed_count'] for sample in samples],
            'all_samples_complete': all(sample['validated']['complete'] for sample in samples),
            'all_samples_same_count': len(set(counts)) == 1,
            'median_internal_seconds': statistics.median(s['internal_seconds'] for s in samples),
            'median_process_seconds': statistics.median(s['process_seconds'] for s in samples),
            'validated': samples[-1]['validated'], 'input': translated,
            'input_sha256': canonical_hash(translated)}


def run_rust(case, binary, folder, args, environment):
    options = {'algorithm': 'auto', 'seed': 0, 'multistart_runs': 4, 'beam_width': 8,
               'guillotine_required': False}
    if case['dimension'] == 'two_d':
        options.update(min_usable_side=0, auto_rotation_search_max_types=4)
    else:
        options.update(improvement_rounds=4, auto_exact_max_types=8,
                       auto_exact_max_quantity=16, branch_and_bound_node_limit=10_000)
    request = {'dimension': case['dimension'], 'problem': case['problem'], 'options': options,
               'warmups': args.warmups, 'repeats': args.repeats, 'max_states': args.max_states}
    encoded = json.dumps(request)
    (folder / 'input.json').write_text(encoded + '\n')
    result, wall = run_process([str(binary)], folder, environment,
                              (args.repeats + args.warmups) * 60, encoded)
    output = json.loads(result.stdout)
    require(output.get('release_build') is True, 'Rust benchmark must be built in release mode')
    expected = ['ordinary', 'adaptive', 'constrained'] if case['dimension'] == 'two_d' else [
        'ordinary', 'constrained']
    require([v.get('variant') for v in output.get('variants', [])] == expected,
            'missing or duplicate Rust variants')
    for variant in output['variants']:
        variant['validated'] = validate_rust_result(case, variant['result'])
        require(len(variant['samples_ns']) == args.repeats
                and all(type(ns) is int and ns > 0 for ns in variant['samples_ns']),
                'invalid Rust timing samples')
        # The Rust harness reports an upper median; recompute the ordinary
        # statistical median so even repeat counts are comparable too.
        variant['median_seconds'] = statistics.median(variant['samples_ns']) / 1_000_000_000
    return {'process_seconds_all_variants': wall, 'request': request,
            'request_sha256': canonical_hash(request), 'full_result': output}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rust-binary', required=True, type=Path)
    parser.add_argument('--rectangle-binary', required=True, type=Path)
    parser.add_argument('--box-binary', required=True, type=Path)
    parser.add_argument('--packingsolver-source', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--warmups', type=int, default=1)
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--time-limit', type=float, default=2.0)
    parser.add_argument('--lp-solver', choices=('highs', 'clp'), default='highs',
                        help='Select an LP solver actually enabled in the reference build')
    parser.add_argument('--max-states', type=int, default=512)
    parser.add_argument('--case', action='append', help='Run only the named generated case; repeatable')
    parser.add_argument('--public-data', type=Path,
                        help='Pinned PackingSolver data directory; add eight fixed public geometry fixtures')
    args = parser.parse_args()
    require(1 <= args.warmups <= 10 and 1 <= args.repeats <= 51, 'invalid sample counts')
    require(math.isfinite(args.time_limit) and 0 < args.time_limit <= 60, 'invalid time limit')
    require(0 <= args.max_states <= 100_000, 'invalid state budget')
    scratch = Path('/Volumes/Scratch')
    require(scratch.is_mount(), 'Scratch is not mounted')
    output_path = args.output.resolve()
    require(output_path.is_relative_to(scratch), 'raw evidence must be written under Scratch')
    require(not output_path.exists(), 'refusing to replace an existing benchmark report')
    for field in ('rust_binary', 'rectangle_binary', 'box_binary'):
        binary = getattr(args, field).resolve()
        require(binary.is_file() and os.access(binary, os.X_OK), 'missing executable ' + str(binary))
        setattr(args, field, binary)
    upstream_revision = command_output(['git', 'rev-parse', 'HEAD'], args.packingsolver_source)
    require(upstream_revision == UPSTREAM_REVISION, 'PackingSolver source revision is not pinned')
    require(not command_output(['git', 'status', '--porcelain'], args.packingsolver_source),
            'PackingSolver source must be clean')
    cases = generated_cases()
    if args.public_data:
        require(args.public_data.resolve() == (args.packingsolver_source / 'data').resolve(),
                'public data must come from the clean pinned source checkout')
        cases += public_cases(args.public_data)
    if args.case:
        require(set(args.case) <= {case['name'] for case in cases}, 'unknown requested case')
        cases = [case for case in cases if case['name'] in args.case]
    output_path.parent.mkdir(parents=True, exist_ok=True)
    artifacts = output_path.parent / (output_path.stem + '-artifacts')
    artifacts.mkdir()
    environment = dict(os.environ, RAYON_NUM_THREADS='1', OMP_NUM_THREADS='1',
                       TMPDIR=str(artifacts), PYTHONDONTWRITEBYTECODE='1')
    metadata = source_metadata()
    binary_paths = {name: str(getattr(args, name)) for name in
                    ('rust_binary', 'rectangle_binary', 'box_binary')}
    build_cache = args.rectangle_binary.parents[2] / 'CMakeCache.txt'
    build_configuration = None
    if build_cache.is_file():
        build_configuration = [line for line in build_cache.read_text().splitlines()
                               if re.match(r'(PACKINGSOLVER_|CMAKE_BUILD_TYPE:|CMAKE_CXX_COMPILER:|CMAKE_CXX_FLAGS_RELEASE:|CMAKE_HOME_DIRECTORY:)', line)]
    targeted = sum(case['category'].startswith('targeted') for case in cases)
    report = {
        'scope': (str(targeted) + ' targeted cases, '
                  + str(sum(case['category'] == 'synthetic_held_out' for case in cases))
                  + ' fixed synthetic cases and '
                  + str(sum(case['category'] == 'public_geometry' for case in cases))
                  + ' public geometry cases; not a representative production or global superiority claim'),
        'development_feedback': {
            '2d-elongated': 'Initially held out; promoted to targeted regression after its quality loss informed implementation changes',
            '3d-seed-29': 'Initially held out; promoted to targeted regression after its quality loss informed implementation changes',
            '2d-seed-421': 'Prior held-out loss promoted to targeted regression before this completion run',
            'fresh_uninspected_seeds': ['2d-seed-1301', '2d-seed-1699', '3d-seed-1201', '3d-seed-1613'],
        },
        'method': {'warmups': args.warmups, 'repeats': args.repeats,
                   'upstream_mode': 'not-anytime-sequential', 'upstream_time_limit': args.time_limit,
                   'upstream_linear_programming_solver': args.lp_solver,
                   'rust_constrained_max_states': args.max_states,
                   'objective': 'full placement then bin count; single stock fixes waste and cost for a given count',
                   'constraints': 'single stock; zero kerf; no defects, trims, weight or support constraints',
                   'timing': 'Rust internal solve includes cloning, excludes parse/serialize; upstream own Time and whole subprocess time recorded separately; different search budgets are explicit',
                   'time_to_equal_quality_measured': False,
                   'partial_reference_policy': 'Validate partial/empty cutoff incumbents with exact source residuals and summary checks; compare bin counts only when every reference sample is complete',
                   'axis_mapping': 'Rust width,height,depth equals upstream x,y,z; no gravity or load constraints'},
        'created_utc': datetime.now(timezone.utc).isoformat(),
        'hardware': {'platform': platform.platform(), 'machine': platform.machine(),
                     'logical_cpus': os.cpu_count(), 'rayon_threads': 1, 'omp_threads': 1},
        'host_load_average_start': list(os.getloadavg()),
        'toolchain': {'rustc': command_output(['rustc', '--version']),
                      'python': sys.version, 'cxx': command_output(['c++', '--version']).splitlines()[0]},
        'source': metadata, 'upstream_git_revision': upstream_revision,
        'upstream_build_configuration': build_configuration,
        'binary_paths': binary_paths,
        'binaries_sha256': {name: file_hash(getattr(args, name)) for name in
                            ('rust_binary', 'rectangle_binary', 'box_binary')},
        'cases': [], 'failures': [],
    }
    for case in cases:
        print('Measuring ' + case['name'], flush=True)
        folder = artifacts / case['name']
        folder.mkdir()
        row = dict(case, input_sha256=canonical_hash(case['problem']))
        for engine in ('rust', 'packingsolver'):
            target = folder / engine
            target.mkdir()
            try:
                if engine == 'rust':
                    row[engine] = run_rust(case, args.rust_binary, target, args, environment)
                else:
                    binary = args.rectangle_binary if case['dimension'] == 'two_d' else args.box_binary
                    row[engine] = run_upstream(case, binary, target, args, environment)
            except (ValueError, KeyError, OSError, subprocess.SubprocessError) as error:
                failure = {'case': case['name'], 'engine': engine, 'error': str(error)}
                report['failures'].append(failure)
                row[engine] = {'failure': failure}
                print('FAILED ' + engine + ': ' + str(error), file=sys.stderr, flush=True)
        if 'failure' not in row['rust'] and 'failure' not in row['packingsolver']:
            upstream = row['packingsolver']
            row['comparisons'] = [{
                'rust_variant': variant['variant'],
                'rust_bin_count': variant['validated']['bin_count'],
                'upstream_bin_count': upstream['validated']['bin_count'],
                'upstream_sample_bin_counts': upstream['sample_bin_counts'],
                'upstream_sample_placed_counts': upstream['sample_placed_counts'],
                'upstream_complete_in_all_samples': upstream['all_samples_complete'],
                'count_difference_rust_minus_upstream': variant['validated']['bin_count']
                - upstream['validated']['bin_count'] if upstream['all_samples_complete'] else None,
                'rust_median_seconds': variant['median_seconds'],
                'upstream_median_internal_seconds': upstream['median_internal_seconds'],
                'internal_runtime_ratio_upstream_over_rust': upstream['median_internal_seconds']
                / variant['median_seconds'],
                'equal_final_count': upstream['all_samples_complete'] and all(variant['validated']['bin_count'] == count
                                         for count in upstream['sample_bin_counts']),
                'time_to_equal_quality_measured': False,
            } for variant in row['rust']['full_result']['variants']]
        report['cases'].append(row)
        output_path.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    report['source_unchanged_during_run'] = (
        metadata['combined_source_sha256'] == source_metadata()['combined_source_sha256'])
    if not report['source_unchanged_during_run']:
        report['failures'].append({'engine': 'source', 'error': 'Rust source changed during measurements'})
    report['binaries_unchanged_during_run'] = all(
        file_hash(path) == report['binaries_sha256'][name] for name, path in binary_paths.items())
    if not report['binaries_unchanged_during_run']:
        report['failures'].append({'engine': 'binaries', 'error': 'An executable changed during measurements'})
    report['all_cases_validated'] = not report['failures']
    report['host_load_average_end'] = list(os.getloadavg())
    output_path.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    print('Wrote ' + str(output_path), flush=True)
    return 0 if report['all_cases_validated'] else 1


if __name__ == '__main__':
    sys.exit(main())
