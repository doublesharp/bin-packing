#!/usr/bin/env python3
"""Validate native objective, staged-cut and stack results against PackingSolver."""

import argparse
import copy
import csv
from datetime import datetime, timezone
import importlib.util
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys

SPEC = importlib.util.spec_from_file_location('packing_comparison', Path(__file__).with_name('compare-packingsolver.py'))
BASE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BASE)
require = BASE.require


def fixtures():
    """Fixed cases selected before observing the completion benchmark results."""
    cases = []
    for dimension in (1, 2, 3):
        demands = [{'name': 'full', 'quantity': 1}, {'name': 'half', 'quantity': 2}]
        if dimension == 1:
            packing = {'stock': [{'name': 'stock', 'length': 10, 'available': 1, 'cost': 1}],
                       'demands': [dict(d, length=w) for d, w in zip(demands, (10, 5))]}
        elif dimension == 2:
            packing = {'problem': {'sheets': [{'name': 'stock', 'width': 10, 'height': 5, 'quantity': 1, 'cost': 1}],
                                   'demands': [dict(d, width=w, height=5, can_rotate=False) for d, w in zip(demands, (10, 5))]}}
        else:
            packing = {'problem': {'bins': [{'name': 'stock', 'width': 10, 'height': 5, 'depth': 5, 'quantity': 1, 'cost': 1}],
                                   'demands': [dict(d, width=w, height=5, depth=5, allowed_rotations=1) for d, w in zip(demands, (10, 5))]},
                       'item_weights': [8, 4], 'bin_max_weights': [8]}
        for objective in ('knapsack', 'feasibility'):
            selected = copy.deepcopy(packing)
            if objective == 'feasibility':
                source = selected if dimension == 1 else selected['problem']
                source['demands'].pop(0)
                if dimension == 3:
                    selected['item_weights'].pop(0)
            cases.append({'name': f'{dimension}d-{objective}', 'dimension': f'{("one", "two", "three")[dimension-1]}_d_optimized',
                          'family': ('onedimensional', 'rectangle', 'box')[dimension-1], 'kind': 'ordinary',
                          'problem': {'packing': selected, 'profits': [1, 10] if objective == 'knapsack' else [], 'minimum_quantities': []},
                          'options': {'objective': objective, 'packing_max_states': 512}, 'objective': objective})
        if dimension > 1:
            selected = copy.deepcopy(packing)
            selected['problem']['demands'].pop(0)
            selected['problem']['sheets' if dimension == 2 else 'bins'][0]['width'] = 100
            if dimension == 3:
                selected['item_weights'].pop(0)
            cases.append({'name': f'{dimension}d-open-x', 'dimension': f'{("two", "three")[dimension-2]}_d_optimized',
                          'family': ('rectangle', 'box')[dimension-2], 'kind': 'ordinary',
                          'problem': {'packing': selected}, 'options': {'objective': 'open_dimension_x'}, 'objective': 'open_dimension_x'})
    cases.append({'name': '1d-fractional-material-cost', 'dimension': 'one_d', 'family': 'onedimensional', 'kind': 'ordinary',
                  'problem': {'stock': [{'name': 'large', 'length': 10, 'cost': 1}, {'name': 'small', 'length': 5, 'cost': 0.35}],
                              'demands': [{'name': 'part', 'length': 5, 'quantity': 2}]},
                  'options': {'exact_pattern_limit': 4096}, 'objective': 'total_cost'})
    for stages in (2, 3):
        cases.append({'name': f'staged-{stages}-axis-levels', 'dimension': 'staged', 'family': 'rectangleguillotine', 'kind': 'staged',
                      'problem': {'packing': {'problem': {'sheets': [{'name': 'sheet', 'width': 10, 'height': 10, 'cost': 1}],
                                  'demands': [{'name': f'p{i}', 'width': w, 'height': h, 'quantity': q, 'can_rotate': False}
                                              for i, (w, h, q) in enumerate(((4, 10, 1), (6, 6, 1), (3, 4, 2)))]}},
                                  'rules': {'max_stages': stages, 'cut_type': 'exact', 'first_axis': 'vertical'}},
                      'options': {'max_states': 4000}, 'objective': 'bin_count'})
    for name, maximum_above, payload in [('nesting', 8, 20), ('weight-above', 2, 20), ('payload', 8, 12)]:
        cases.append({'name': 'loading-' + name, 'dimension': 'loading', 'family': 'boxstacks', 'kind': 'loading',
                      'problem': {'packing': {'problem': {'bins': [{'name': 'crate', 'width': 4, 'height': 6, 'depth': 4, 'cost': 1}],
                                   'demands': [{'name': 'nested', 'width': 2, 'height': 4, 'depth': 2, 'quantity': 5, 'allowed_rotations': 1}]},
                                   'item_weights': [4], 'bin_max_weights': [payload]},
                                  'item_rules': [{'nesting_height': 2, 'maximum_stackability': 2, 'maximum_weight_above': maximum_above}],
                                  'bin_rules': [{'maximum_stack_density': {'numerator': 2, 'denominator': 1}}]},
                      'options': {'max_states': 4096}, 'objective': 'bin_count'})
    return cases


def physical(case):
    problem = case['problem']
    if case['dimension'].endswith('_optimized'):
        problem = problem['packing']
    if case['kind'] in ('staged', 'loading'):
        problem = problem['packing']
    return problem


def source(case):
    problem = physical(case)
    if 'stock' in problem:
        return problem['stock'], problem['demands'], ['length']
    source_problem = problem['problem']
    if 'sheets' in source_problem:
        return source_problem['sheets'], source_problem['demands'], ['width', 'height']
    return source_problem['bins'], source_problem['demands'], ['width', 'height', 'depth']


def upstream_input(case):
    stocks, demands, axes = source(case)
    objective = {'bin_count': 'bin-packing', 'total_cost': 'variable-sized-bin-packing',
                 'open_dimension_x': 'open-dimension-x'}.get(case['objective'], case['objective'])
    result = {'objective': objective, 'bin_types': [], 'item_types': []}
    profits = case['problem'].get('profits', [])
    quantity = sum(d['quantity'] for d in demands)
    for index, stock in enumerate(stocks):
        cap = stock.get('available' if len(axes) == 1 else 'quantity')
        bin_type = {'cost': stock.get('cost', 1), 'copies': quantity if cap is None else cap}
        if len(axes) == 1:
            bin_type['length'] = stock['length']
        elif case['kind'] == 'staged':
            bin_type.update(width=stock['width'], height=stock['height'])
        else:
            names = ['x', 'y'] if len(axes) == 2 else ['x', 'y', 'z']
            values = [stock[a] for a in axes]
            if case['kind'] == 'loading':
                values[1], values[2] = values[2], values[1]
                rules = case['problem'].get('bin_rules', [{}])[index]
                density = rules.get('maximum_stack_density')
                if density:
                    bin_type['maximum_stack_density'] = density['numerator'] / density['denominator']
            bin_type.update(zip(names, values))
        weights = physical(case).get('bin_max_weights', [])
        if weights:
            bin_type['maximum_weight'] = weights[index]
        result['bin_types'].append(bin_type)
    for index, demand in enumerate(demands):
        item = {'copies': demand['quantity'], 'profit': profits[index] if profits else 1}
        if len(axes) == 1:
            item['length'] = demand['length']
        elif case['kind'] == 'staged':
            item.update(width=demand['width'], height=demand['height'], oriented=not demand['can_rotate'])
        else:
            values = [demand[a] for a in axes]
            names = ['x', 'y'] if len(axes) == 2 else ['x', 'y', 'z']
            if case['kind'] == 'loading':
                values[1], values[2] = values[2], values[1]
                item.update(case['problem'].get('item_rules', [{}])[index])
            item.update(zip(names, values))
            if len(axes) == 2:
                item['oriented'] = not demand['can_rotate']
            else:
                item['rotations'] = ['XYZ'] # Fixed identity is part of these fixtures.
        weights = physical(case).get('item_weights', [])
        if weights:
            item['weight'] = weights[index]
        result['item_types'].append(item)
    if case['kind'] == 'staged':
        rules = case['problem']['rules']
        result.update(number_of_stages=rules['max_stages'], cut_type=rules['cut_type'],
                      first_stage_orientation=rules['first_axis'])
    return result


def csv_rows(path):
    with path.open(newline='') as stream:
        rows = list(csv.DictReader(stream))
    require(rows and all(None not in row and None not in row.values() for row in rows), 'malformed certificate')
    return rows


def native_layouts(case, result):
    outer = result
    result = result.get('solution', result)
    if case['kind'] == 'staged':
        result = outer['solution']
    stocks, demands, axes = source(case)
    layouts = []
    if case['kind'] == 'loading':
        for layout in result['layouts']:
            placements = []
            for stack_index, stack in enumerate(layout['stacks']):
                require(stack['height'] == max(item['y'] + demands[item['demand_index']]['height'] for item in stack['items']), 'wrong native stack height')
                require(stack['weight'] == sum(physical(case)['item_weights'][item['demand_index']] for item in stack['items']), 'wrong native stack weight')
                for item in stack['items']:
                    d = demands[item['demand_index']]
                    require(item['rotation'] == 'xyz', 'fixture permits identity only')
                    placements.append({'item': item['demand_index'], 'position': [stack['x'], item['y'], stack['z']],
                                       'dimensions': [d[a] for a in axes], 'stack': stack_index})
            require(layout['payload'] == sum(stack['weight'] for stack in layout['stacks']), 'wrong native payload total')
            layouts.append({'stock': layout['bin_index'], 'copies': 1, 'placements': placements})
        return layouts
    for index, layout in enumerate(result['layouts']):
        if case['dimension'].endswith('_optimized') or case['kind'] == 'staged':
            stock = outer['stock_indices'][index]
            indices = outer['demand_indices'][index]
        else:
            stock = next(i for i, s in enumerate(stocks) if s['name'] == layout['stock_name'])
            indices = [next(i for i, d in enumerate(demands) if d['name'] == cut['name']) for cut in layout['cuts']]
        placements = []
        cursor = 0
        for item_index, placement in enumerate(layout['cuts' if len(axes) == 1 else 'placements']):
            if len(axes) == 1:
                position, size = [cursor], [placement['length']]
                cursor += placement['length']
            else:
                position = [placement[a] for a in ['x', 'y', 'z'][:len(axes)]]
                size = [placement[a] for a in axes]
            placements.append({'item': indices[item_index], 'position': position, 'dimensions': size})
        layouts.append({'stock': stock, 'copies': 1, 'placements': placements})
    return layouts


def reference_layouts(case, rows):
    stocks, _, axes = source(case)
    layouts = {}
    if case['kind'] == 'staged':
        for row in rows:
            if row['PARENT'] == '' and int(row['NODE_ID']) >= 0:
                require(int(row['PLATE_ID']) not in layouts, 'duplicate root')
                layouts[int(row['PLATE_ID'])] = {'stock': 0, 'copies': int(row['COPIES']), 'placements': []}
        children = {int(row['PARENT']) for row in rows if row['PARENT']}
        for row in rows:
            if int(row['NODE_ID']) not in children and int(row['TYPE']) >= 0:
                layouts[int(row['PLATE_ID'])]['placements'].append({'item': int(row['TYPE']), 'position': [int(row['X']), int(row['Y'])],
                    'dimensions': [int(row['WIDTH']), int(row['HEIGHT'])]})
        return list(layouts.values())
    for row in rows:
        if row['TYPE'] == 'BIN':
            require(int(row['BIN']) not in layouts, 'duplicate reference bin')
            index = int(row['ID'])
            require(0 <= index < len(stocks), 'unknown bin type')
            certificate = [int(row[k]) for k in ['LX', 'LY', 'LZ'][:len(axes)]]
            expected = [stocks[index][a] for a in axes]
            if case['kind'] == 'loading':
                certificate[1], certificate[2] = certificate[2], certificate[1]
            require(certificate == expected, 'incorrect reference bin dimensions')
            layouts[int(row['BIN'])] = {'stock': index, 'copies': int(row['COPIES']), 'placements': []}
    for row in rows:
        require(row['TYPE'] in ('BIN', 'ITEM', 'STACK'), 'unexpected certificate record')
        if row['TYPE'] != 'ITEM':
            continue
        layout = layouts[int(row['BIN'])]
        require(int(row['COPIES']) == layout['copies'], 'copy-count mismatch')
        position = [int(row[k]) for k in ['X', 'Y', 'Z'][:len(axes)]]
        size = [int(row[k]) for k in ['LX', 'LY', 'LZ'][:len(axes)]]
        if case['kind'] == 'loading':
            position[1], position[2] = position[2], position[1]
            size[1], size[2] = size[2], size[1]
        if len(axes) == 3:
            require(row['ROTATION'] == 'XYZ', 'fixture permits identity rotation only')
        placement = {'item': int(row['ID']), 'position': position, 'dimensions': size}
        if case['kind'] == 'loading':
            placement['stack'] = int(row['STACK'])
        layout['placements'].append(placement)
    return list(layouts.values())


def validate_geometry(case, layouts):
    stocks, demands, axes = source(case)
    counts, usage = [0] * len(demands), [0] * len(stocks)
    cost, extents = 0, [0] * len(axes)
    for layout in layouts:
        stock_index, copies = layout['stock'], BASE.integer(layout['copies'], 'copies', 1)
        require(0 <= stock_index < len(stocks), 'unknown stock')
        stock = stocks[stock_index]
        usage[stock_index] += copies
        cost += stock.get('cost', 1) * copies
        placements = layout['placements']
        require(placements, 'empty container')
        payload = 0
        for index, placement in enumerate(placements):
            item = BASE.integer(placement['item'], 'item')
            require(item < len(demands), 'unknown demand')
            demand = demands[item]
            position, size = placement['position'], placement['dimensions']
            require(all(type(v) is int and v >= 0 for v in position), 'invalid coordinates')
            require(size == [demand[a] for a in axes], 'changed dimensions or forbidden rotation')
            require(all(p + s <= stock[a] for p, s, a in zip(position, size, axes)), 'outside stock')
            for axis, (p, s) in enumerate(zip(position, size)):
                extents[axis] = max(extents[axis], p + s)
            for other in placements[:index]:
                if case['kind'] == 'loading' and placement['stack'] == other['stack']:
                    continue # Verified against exact authorized nesting below.
                require(any(p + s <= q or q + t <= p for p, s, q, t in zip(position, size, other['position'], other['dimensions'])), 'overlapping items')
            counts[item] += copies
            payload += physical(case).get('item_weights', [0] * len(demands))[item]
        caps = physical(case).get('bin_max_weights', [])
        require(not caps or caps[stock_index] is None or payload <= caps[stock_index], 'payload exceeded')
        if case['kind'] == 'loading':
            validate_stacks(case, placements, stock_index)
    for index, stock in enumerate(stocks):
        cap = stock.get('available' if len(axes) == 1 else 'quantity')
        require(cap is None or usage[index] <= cap, 'stock inventory exceeded')
    require(all(n <= d['quantity'] for n, d in zip(counts, demands)), 'excess demand copies')
    complete = counts == [d['quantity'] for d in demands]
    if case['objective'] != 'knapsack':
        require(complete, 'comparison requires a complete packing')
    profits = case['problem'].get('profits', []) or [1] * len(demands)
    profit = sum(n * p for n, p in zip(counts, profits))
    return {'bin_count': sum(usage), 'cost': cost, 'counts': counts, 'profit': profit,
            'placed': sum(counts), 'complete': complete, 'extents': extents}


def validate_stacks(case, placements, stock):
    _, demands, _ = source(case)
    weights = physical(case)['item_weights']
    groups = {}
    for placement in placements:
        groups.setdefault(placement['stack'], []).append(placement)
    for items in groups.values():
        items.sort(key=lambda p: p['position'][1])
        height, weight = 0, 0
        footprint = None
        stackability = None
        for index, item in enumerate(items):
            rule = case['problem']['item_rules'][item['item']]
            x, y, z = item['position']
            w, h, d = item['dimensions']
            require(footprint is None or footprint == (x, z, w, d), 'mismatched stack footprint')
            footprint = (x, z, w, d)
            group = rule.get('stackability_id', 0)
            require(stackability is None or stackability == group, 'incompatible stackability')
            stackability = group
            bottom = 0 if index == 0 else height - rule.get('nesting_height', 0)
            require(y == bottom, 'unsupported or unauthorized nesting')
            height = y + h
            weight += weights[item['item']]
            limit = rule.get('maximum_stackability')
            require(limit is None or len(items) <= limit, 'stack-count limit exceeded')
            above = sum(weights[upper['item']] for upper in items[index + 1:])
            require(rule.get('maximum_weight_above') is None or above <= rule['maximum_weight_above'], 'weight-above limit exceeded')
        density = case['problem']['bin_rules'][stock].get('maximum_stack_density')
        if density:
            require(weight * density['denominator'] <= density['numerator'] * footprint[2] * footprint[3], 'density limit exceeded')


def validate_tree_partition(region, children):
    x, y, w, h = region
    if len(children) == 1 and children[0] == region:
        return None
    vertical = all(c[1] == y and c[3] == h for c in children)
    horizontal = all(c[0] == x and c[2] == w for c in children)
    require(vertical != horizontal, 'children are not full-span cuts')
    axis = 0 if vertical else 1
    ordered = sorted(children, key=lambda c: c[axis])
    cursor = region[axis]
    for child in ordered:
        require(child[axis] == cursor, 'unaccounted material or overlapping cut children')
        cursor += child[axis + 2]
    require(cursor == region[axis] + region[axis + 2], 'incomplete material partition')
    return axis


def validate_staged(case, result=None, rows=None):
    """Independent zero-kerf exact-stage tree partition check for these fixtures."""
    maximum = case['problem']['rules']['max_stages']
    stocks, demands, _ = source(case)
    trees = []
    if result is not None:
        for tree in result['trees']:
            nodes = {}
            for index, node in enumerate(tree['nodes']):
                r, action = node['region'], node['action']
                nodes[index] = {'region': (r['x'], r['y'], r['width'], r['height']),
                                'children': action.get('children', []),
                                'item': action.get('demand_index') if action['kind'] == 'part' else None}
            trees.append((0, nodes))
    else:
        by_plate = {}
        for row in rows:
            require(int(row['NODE_ID']) >= 0, 'unexpected fixture defect')
            nodes = by_plate.setdefault(int(row['PLATE_ID']), {})
            index = int(row['NODE_ID'])
            require(index not in nodes, 'duplicate cut node')
            nodes[index] = {'region': tuple(int(row[k]) for k in ('X', 'Y', 'WIDTH', 'HEIGHT')),
                            'children': [], 'item': int(row['TYPE']) if int(row['TYPE']) >= 0 else None,
                            'parent': int(row['PARENT']) if row['PARENT'] else None}
        for nodes in by_plate.values():
            roots = []
            for index, node in nodes.items():
                if node['parent'] is None:
                    roots.append(index)
                else:
                    nodes[node['parent']]['children'].append(index)
            require(len(roots) == 1, 'wrong tree root count')
            trees.append((roots[0], nodes))
    for tree_index, (root, nodes) in enumerate(trees):
        require(nodes[root]['region'] == (0, 0, stocks[0]['width'], stocks[0]['height']), 'wrong root stock')
        seen, frontier, leaves = set(), [(root, None, 0)], []
        while frontier:
            index, previous, stage = frontier.pop()
            require(index in nodes and index not in seen, 'cycle, alias or missing cut node')
            seen.add(index)
            node = nodes[index]
            require(all(v > 0 for v in node['region'][2:]), 'nonpositive material region')
            children = node['children']
            if children:
                axis = validate_tree_partition(node['region'], [nodes[c]['region'] for c in children])
                if axis is not None:
                    stage = (1 if axis == 0 else 2) if previous is None else stage + int(axis != previous)
                    require(stage <= maximum, 'physical stage limit exceeded')
                    previous = axis
                frontier.extend((c, previous, stage) for c in children)
            elif node['item'] is not None:
                item = node['item']
                require(0 <= item < len(demands), 'unknown finished part')
                require(node['region'][2:] == (demands[item]['width'], demands[item]['height']), 'finished part does not fill its region')
                leaves.append((item, *node['region']))
        require(len(seen) == len(nodes), 'unreachable cut nodes')
        if result is not None:
            layout = native_layouts(case, result)[tree_index]
            placed = [(p['item'], *p['position'], *p['dimensions']) for p in layout['placements']]
            require(sorted(placed) == sorted(leaves), 'layout differs from physical cut tree')


def primary(case, checked):
    return checked['profit'] if case['objective'] == 'knapsack' else checked['cost'] if case['objective'] == 'total_cost' else checked['extents'][0] if case['objective'] == 'open_dimension_x' else checked['bin_count']


def validate_native(case, result):
    checked = validate_geometry(case, native_layouts(case, result))
    actual = result['solution'] if case['dimension'].endswith('_optimized') else result
    if case['kind'] == 'staged':
        validate_staged(case, result=result)
        actual = result['solution']
    require(math.isclose(actual['total_cost'], checked['cost']), 'wrong native material total')
    if case['dimension'].endswith('_optimized'):
        report = result['report']
        require(report['selected_quantities'] == checked['counts'], 'wrong source quantities')
        require(report['complete'] == checked['complete'], 'wrong completeness report')
        require(report['total_profit'] == checked['profit'], 'wrong profit report')
        if case['objective'] == 'open_dimension_x':
            require(report['open_extents'][0] >= checked['extents'][0], 'open crop excludes items')
            checked['extents'] = report['open_extents']
    return checked


def validate_reference(case, path, payload):
    rows = csv_rows(path)
    checked = validate_geometry(case, reference_layouts(case, rows))
    if case['kind'] == 'staged':
        validate_staged(case, rows=rows)
    summary = payload['Output']['Solution']
    for key, value in [('NumberOfItems', checked['placed']), ('NumberOfBins', checked['bin_count']), ('BinCost', checked['cost']), ('ItemProfit', checked['profit'])]:
        require(math.isclose(summary[key], value), 'wrong upstream summary ' + key)
    if case['objective'] == 'open_dimension_x':
        require(math.isclose(summary['XMax'], checked['extents'][0]), 'wrong upstream open extent')
    return checked


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rust-binary', type=Path, required=True)
    parser.add_argument('--reference-build', type=Path, required=True)
    parser.add_argument('--reference-source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--time-limit', type=float, default=2)
    parser.add_argument('--case', action='append')
    args = parser.parse_args()
    require(Path('/Volumes/Scratch').is_mount(), 'Scratch is not mounted')
    require(args.output.resolve().is_relative_to(Path('/Volumes/Scratch')), 'evidence must be on Scratch')
    require(not args.output.exists(), 'refusing to replace evidence')
    require(1 <= args.repeats <= 51 and math.isfinite(args.time_limit) and 0 < args.time_limit <= 60, 'invalid effort controls')
    require(BASE.command_output(['git', 'rev-parse', 'HEAD'], args.reference_source) == BASE.UPSTREAM_REVISION, 'wrong reference revision')
    require(not BASE.command_output(['git', 'status', '--porcelain'], args.reference_source), 'dirty reference source')
    cases = fixtures()
    if args.case:
        require(set(args.case) <= {case['name'] for case in cases}, 'unknown case')
        cases = [case for case in cases if case['name'] in args.case]
    artifact = args.output.with_name(args.output.stem + '-artifacts')
    artifact.mkdir(parents=True)
    environment = dict(os.environ, TMPDIR=str(artifact), RAYON_NUM_THREADS='1', OMP_NUM_THREADS='1')
    metadata = BASE.source_metadata()
    binaries = {'rust': args.rust_binary, **{case['family']: args.reference_build / 'src' / case['family'] / ('packingsolver_' + case['family']) for case in cases}}
    hashes = {name: BASE.file_hash(path) for name, path in binaries.items()}
    report = {'created_utc': datetime.now(timezone.utc).isoformat(), 'source': metadata,
              'reference_revision': BASE.UPSTREAM_REVISION, 'binaries_sha256': hashes,
              'script_sha256': BASE.file_hash(Path(__file__)), 'host_load_start': list(os.getloadavg()),
              'method': {'warmups': 1, 'repeats': args.repeats, 'reference_seconds': args.time_limit,
                         'reference_mode': 'not-anytime-sequential', 'reference_lp': 'highs',
                         'budgets_equal': False, 'time_to_equal_quality_measured': False,
                         'scope': 'fixed targeted capability cases; independent certificate readback; no global performance claim'},
              'cases': [], 'failures': []}
    for case in cases:
        print('Comparing ' + case['name'], flush=True)
        folder = artifact / case['name']
        folder.mkdir()
        row = dict(case, input_sha256=BASE.canonical_hash(case))
        try:
            request = {'dimension': case['dimension'], 'problem': case['problem'], 'options': case['options'], 'warmups': 1, 'repeats': args.repeats, 'max_states': 4000}
            run, wall = BASE.run_process([str(args.rust_binary)], folder, environment, 180, json.dumps(request))
            native = json.loads(run.stdout)
            require(native['release_build'], 'native executable must be release')
            variant = native['variants'][0]
            checked = validate_native(case, variant['result'])
            row['rust'] = {'validated': checked, 'primary': primary(case, checked), 'median_internal_seconds': statistics.median(variant['samples_ns']) / 1e9, 'process_seconds': wall, 'full_result': native}
            translated = upstream_input(case)
            (folder / 'reference-input.json').write_text(json.dumps(translated, indent=2) + '\n')
            samples = []
            for sample in range(args.repeats + 1):
                target = folder / f'reference-{sample}'
                target.mkdir()
                command = [str(binaries[case['family']]), '--input', str(folder / 'reference-input.json'), '--linear-programming-solver', 'highs', '--optimization-mode', 'not-anytime-sequential', '--time-limit', str(args.time_limit), '--verbosity-level', '0', '--only-write-at-the-end', '--output', str(target / 'output.json'), '--certificate', str(target / 'certificate.csv')]
                _, wall = BASE.run_process(command, target, environment, args.time_limit + 30)
                payload = json.loads((target / 'output.json').read_text())
                checked = validate_reference(case, target / 'certificate.csv', payload)
                if sample:
                    samples.append({'validated': checked, 'primary': primary(case, checked), 'internal_seconds': payload['Output']['Time'], 'process_seconds': wall, 'command': command, 'full_result': payload})
            row['reference'] = {'input': translated, 'samples': samples, 'median_internal_seconds': statistics.median(s['internal_seconds'] for s in samples), 'sample_primary_values': [s['primary'] for s in samples]}
            row['all_primary_values_match'] = all(s['primary'] == row['rust']['primary'] for s in samples)
        except (OSError, ValueError, KeyError, StopIteration, subprocess.SubprocessError) as error:
            failure = {'case': case['name'], 'error': str(error)}
            row['failure'] = failure
            report['failures'].append(failure)
            print('FAILED: ' + str(error), file=sys.stderr, flush=True)
        report['cases'].append(row)
        args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    report['source_unchanged'] = metadata['combined_source_sha256'] == BASE.source_metadata()['combined_source_sha256']
    report['binaries_unchanged'] = all(BASE.file_hash(path) == hashes[name] for name, path in binaries.items())
    report['host_load_end'] = list(os.getloadavg())
    report['all_cases_validated'] = not report['failures'] and report['source_unchanged'] and report['binaries_unchanged']
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    return 0 if report['all_cases_validated'] else 1


if __name__ == '__main__':
    sys.exit(main())
