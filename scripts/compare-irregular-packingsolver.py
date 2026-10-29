#!/usr/bin/env python3
"""Compare pinned irregular solvers with independent line/circular-arc readback.

Only matching source semantics are scored. Geometry validation uses analytic
vertical arrangements and primitive distances, independently of either solver.
"""

import argparse
from collections import namedtuple
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import time


UPSTREAM_REVISION = '376b8d1cc4aed5b171bfe1219c037c661faaa7cf'
ROOT = Path(os.environ.get('BIN_PACKING_SOURCE', Path(__file__).resolve().parents[1])).resolve()
Edge = namedtuple('Edge', 'start end center delta')
OBJECTIVES = {'bin-packing': 'bin_count', 'variable-sized-bin-packing': 'total_cost',
              'knapsack': 'knapsack', 'feasibility': 'feasibility',
              'open-dimension-x': 'open_dimension_x', 'open-dimension-y': 'open_dimension_y'}
GEOMETRY_KEYS = {'type', 'width', 'height', 'x', 'y', 'radius', 'vertices', 'elements',
                 'is_path', 'holes'}


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def numeric(value, label):
    require(type(value) in (int, float) and math.isfinite(value), 'invalid numeric ' + label)
    return value


def integer(value, label, minimum=0):
    require(type(value) is int and value >= minimum, 'invalid integer ' + label)
    return value


def point(value):
    return numeric(value.get('x'), 'point.x'), numeric(value.get('y'), 'point.y')


def point_json(value):
    return dict(zip(('x', 'y'), value))


def distance(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])


def close_number(actual, expected, label, tolerance=1e-7):
    require(math.isclose(numeric(actual, label), expected, rel_tol=1e-9, abs_tol=tolerance),
            'mismatched ' + label)


def allowed_angle(edge, angle):
    if edge.center is None:
        return False
    initial = math.atan2(edge.start[1] - edge.center[1], edge.start[0] - edge.center[0])
    traveled = ((initial - angle) if edge.delta < 0 else (angle - initial)) % math.tau
    return traveled <= abs(edge.delta) + 1e-10 or math.tau - traveled <= 1e-10


def arc(start, end, center, clockwise=False):
    radius = distance(start, center)
    require(radius > 0 and math.isclose(radius, distance(end, center), rel_tol=1e-9, abs_tol=1e-9),
            'invalid arc radius')
    initial = math.atan2(start[1] - center[1], start[0] - center[0])
    final = math.atan2(end[1] - center[1], end[0] - center[0])
    sweep = math.tau if start == end else ((initial - final) if clockwise else (final - initial)) % math.tau
    return Edge(start, end, center, -sweep if clockwise else sweep)


def contour_edges(contour):
    kind = contour.get('kind')
    if kind == 'polygon':
        vertices = [point(p) for p in contour['vertices']]
        if vertices and vertices[0] == vertices[-1]:
            vertices.pop()
        require(3 <= len(vertices) <= 2048, 'invalid polygon vertices')
        return [Edge(a, b, None, 0) for a, b in zip(vertices, vertices[1:] + vertices[:1])]
    if kind == 'circle':
        center = point(contour['center'])
        radius = numeric(contour.get('radius'), 'radius')
        require(radius > 0, 'nonpositive circle radius')
        start = center[0] + radius, center[1]
        return [arc(start, start, center)]
    require(kind == 'curve', 'unknown contour kind')
    start = current = point(contour['start'])
    edges = []
    for segment in contour['segments']:
        end = point(segment['end'])
        require(segment['kind'] in ('line', 'arc'), 'unknown curve segment')
        edges.append(Edge(current, end, None, 0) if segment['kind'] == 'line' else
                     arc(current, end, point(segment['center']), segment['clockwise']))
        current = end
    require(edges and current == start, 'unclosed curve')
    return edges


def shape_geometry(shape):
    require(isinstance(shape.get('components'), list) and shape['components'], 'empty source shape')
    return [[contour_edges(c['exterior'])] + [contour_edges(h) for h in c.get('holes', [])]
            for c in shape['components']]


def edges(geometry):
    return [edge for component in geometry for ring in component for edge in ring]


def extrema(edge):
    result = [edge.start, edge.end]
    if edge.center is not None:
        radius = distance(edge.start, edge.center)
        for angle, dx, dy in [(0, 1, 0), (math.pi / 2, 0, 1),
                              (math.pi, -1, 0), (3 * math.pi / 2, 0, -1)]:
            if allowed_angle(edge, angle):
                result.append((edge.center[0] + dx * radius, edge.center[1] + dy * radius))
    return result


def bounds(geometry):
    points = [p for edge in edges(geometry) for p in extrema(edge)]
    return (min(p[0] for p in points), min(p[1] for p in points),
            max(p[0] for p in points), max(p[1] for p in points))


def ring_area(ring):
    origin = ring[0].start
    value = 0.0
    for edge in ring:
        a, b = edge.start, edge.end
        if edge.center is None:
            value += (a[0] - origin[0]) * (b[1] - origin[1]) - (b[0] - origin[0]) * (a[1] - origin[1])
        else:
            center = edge.center
            value += ((center[0] - origin[0]) * (b[1] - a[1])
                      - (center[1] - origin[1]) * (b[0] - a[0])
                      + distance(a, center) ** 2 * edge.delta)
    return abs(value) / 2


def area(geometry):
    return sum(ring_area(c[0]) - sum(ring_area(h) for h in c[1:]) for c in geometry)


def transform(geometry, angle, mirrored, x, y):
    if angle % 90 == 0:
        cosine, sine = [(1, 0), (0, 1), (-1, 0), (0, -1)][int(angle / 90) % 4]
    else:
        cosine, sine = math.cos(math.radians(angle)), math.sin(math.radians(angle))

    def apply(p):
        px = -p[0] if mirrored else p[0]
        return px * cosine - p[1] * sine + x, px * sine + p[1] * cosine + y

    return [[[Edge(apply(e.start), apply(e.end), None if e.center is None else apply(e.center),
                   -e.delta if mirrored else e.delta) for e in ring] for ring in c] for c in geometry]


def contour_from_upstream(value):
    kind = value.get('type')
    if kind == 'rectangle':
        x, y = value.get('x', 0), value.get('y', 0)
        w, h = numeric(value.get('width'), 'width'), numeric(value.get('height'), 'height')
        require(w > 0 and h > 0, 'nonpositive rectangle')
        return {'kind': 'polygon', 'vertices': [point_json(p) for p in
                [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]]}
    if kind == 'polygon':
        return {'kind': 'polygon', 'vertices': [point_json(point(p)) for p in value['vertices']]}
    if kind == 'circle':
        return {'kind': 'circle', 'center': {'x': value.get('x', 0), 'y': value.get('y', 0)},
                'radius': value['radius']}
    require(kind == 'general' and not value.get('is_path', False), 'unsupported source shape type/path')
    segments, start, current = [], None, None
    for element in value['elements']:
        beginning, end = point(element['start']), point(element['end'])
        require(current is None or beginning == current, 'disconnected source curve')
        if start is None:
            start = beginning
        if element['type'] == 'LineSegment':
            segments.append({'kind': 'line', 'end': point_json(end)})
        else:
            require(element['type'] == 'CircularArc', 'unknown source element')
            orientation = element['orientation'].lower()
            require(orientation in ('clockwise', 'anticlockwise', 'full'), 'unknown arc orientation')
            require(orientation != 'full' or beginning == end, 'non-closed full circle')
            segments.append({'kind': 'arc', 'end': point_json(end),
                             'center': point_json(point(element['center'])),
                             'clockwise': orientation == 'clockwise'})
        current = end
    result = {'kind': 'curve', 'start': point_json(start), 'segments': segments}
    # Straight general contours and polygons have identical canonical geometry.
    if all(s['kind'] == 'line' for s in segments):
        result = {'kind': 'polygon', 'vertices': [point_json(point(e['start'])) for e in value['elements']]}
    contour_edges(result)
    return result


def component_from_upstream(value):
    return {'exterior': contour_from_upstream(value),
            'holes': [contour_from_upstream(h) for h in value.get('holes', [])]}


def normalize_upstream(payload, name='fixture'):
    require(set(payload) <= {'objective', 'parameters', 'bin_types', 'item_types'}, 'unsupported instance fields')
    objective = str(payload.get('objective', '')).lower()
    require(objective in OBJECTIVES, 'objective has no matching native semantics: ' + objective)
    parameters = payload.get('parameters', {})
    require(set(parameters) <= {'item_item_minimum_spacing'}, 'unsupported parameter semantics')
    options = {'objective': OBJECTIVES[objective],
               'item_spacing': parameters.get('item_item_minimum_spacing', 0),
               'bin_spacing': 0, 'defect_spacing': 0}
    bins, demands, defect_spacings = [], [], set()
    for index, value in enumerate(payload['bin_types']):
        require(set(value) <= GEOMETRY_KEYS | {'copies', 'copies_min', 'cost', 'defects',
                                               'item_bin_minimum_spacing'}, 'unsupported bin fields')
        require(value.get('copies_min', 0) == 0 and not value.get('holes'), 'unsupported mandatory bin/bin holes')
        shape = {'components': [component_from_upstream(value)]}
        defects = []
        for defect in value.get('defects', []):
            require(set(defect) <= GEOMETRY_KEYS | {'defect_type', 'item_defect_minimum_spacing'},
                    'unsupported defect fields')
            require(defect.get('defect_type', -1) == -1, 'typed defects have no shared quality semantics')
            defect_spacings.add(numeric(defect.get('item_defect_minimum_spacing', 0), 'defect spacing'))
            defects.append({'components': [component_from_upstream(defect)]})
        copies = value.get('copies', 1)
        require(type(copies) is int and copies >= -1, 'invalid source bin copies')
        material_area = area(shape_geometry(shape))
        cost = value.get('cost', material_area)
        bins.append({'name': 'stock_' + str(index), 'shape': shape,
                     'quantity': None if copies == -1 else copies,
                     'cost': material_area if cost == -1 else cost,
                     'spacing': value.get('item_bin_minimum_spacing', 0), 'defects': defects})
    require(len(defect_spacings) <= 1, 'per-defect spacing does not match a global native defect spacing')
    options['defect_spacing'] = next(iter(defect_spacings), 0)
    for index, value in enumerate(payload['item_types']):
        require(set(value) <= GEOMETRY_KEYS | {'shapes', 'copies', 'copies_min', 'profit',
                                               'allowed_rotations', 'allow_mirroring'}, 'unsupported item fields')
        require(value.get('copies_min', -1) == -1, 'unsupported item minimum copies')
        components = value.get('shapes', [value])
        shape = {'components': [component_from_upstream(c) for c in components]}
        rotations = value.get('allowed_rotations', [{'start': 0, 'end': 0}])
        normal = {(r['start'], r['end']) for r in rotations if not r.get('mirror', False)}
        mirrored = {(r['start'], r['end']) for r in rotations if r.get('mirror', False)}
        if value.get('allow_mirroring', False):
            mirrored |= normal
        require(normal and (not mirrored or mirrored == normal), 'per-angle mirror permissions cannot map exactly')
        demands.append({'name': 'part_' + str(index), 'shape': shape,
                        'quantity': integer(value.get('copies', 1), 'item copies', 1),
                        'profit': value.get('profit', area(shape_geometry(shape))),
                        'can_mirror': bool(mirrored),
                        'allowed_rotations': [{'start': a, 'end': b} for a, b in sorted(normal)]})
    require(bins and demands and sum(d['quantity'] for d in demands) <= 64, 'unsupported source cardinality')
    for stock in bins:
        require(numeric(stock['cost'], 'bin cost') > 0, 'upstream requires positive stock costs')
        require(numeric(stock['spacing'], 'boundary spacing') >= 0, 'negative boundary spacing')
    require(all(numeric(options[k], k) >= 0 for k in ('item_spacing', 'bin_spacing', 'defect_spacing')),
            'negative material clearance')
    for demand in demands:
        require(numeric(demand['profit'], 'profit') >= 0, 'negative source profit')
        require(all(0 <= numeric(r['start'], 'angle') <= numeric(r['end'], 'angle') <= 360
                    for r in demand['allowed_rotations']), 'invalid source rotations')
    return {'name': name, 'category': 'targeted', 'dimension': 'irregular',
            'problem': {'bins': bins, 'demands': demands}, 'options': options}


def contour_to_upstream(contour):
    if contour['kind'] == 'polygon':
        return {'type': 'polygon', 'vertices': contour['vertices']}
    if contour['kind'] == 'circle':
        return dict(type='circle', radius=contour['radius'], **contour['center'])
    output, current = [], contour['start']
    for segment in contour['segments']:
        element = {'type': 'LineSegment' if segment['kind'] == 'line' else 'CircularArc',
                   'start': current, 'end': segment['end']}
        if segment['kind'] == 'arc':
            element.update(center=segment['center'], orientation=(
                'Clockwise' if segment['clockwise'] else 'Anticlockwise'))
            if current == segment['end'] and not segment['clockwise']:
                element['orientation'] = 'Full'
        output.append(element)
        current = segment['end']
    return {'type': 'general', 'elements': output, 'is_path': False}


def component_to_upstream(component):
    value = contour_to_upstream(component['exterior'])
    if component.get('holes'):
        value['holes'] = [contour_to_upstream(h) for h in component['holes']]
    return value


def upstream_input(case):
    problem, options = case['problem'], case['options']
    objective = next((k for k, v in OBJECTIVES.items() if v == options['objective']), None)
    require(objective is not None, 'unsupported native objective')
    stocks, items = [], []
    for stock in problem['bins']:
        require(len(stock['shape']['components']) == 1 and not stock['shape']['components'][0]['holes'],
                'upstream bins cannot represent holes/multiple material components')
        value = component_to_upstream(stock['shape']['components'][0])
        value.update(copies=-1 if stock['quantity'] is None else stock['quantity'], cost=stock['cost'],
                     item_bin_minimum_spacing=max(options.get('bin_spacing', 0), stock['spacing']))
        value['defects'] = []
        for defect in stock['defects']:
            require(len(defect['components']) == 1, 'compound defect lacks matching upstream representation')
            value['defects'].append(dict(component_to_upstream(defect['components'][0]),
                                         item_defect_minimum_spacing=options['defect_spacing']))
        stocks.append(value)
    for demand in problem['demands']:
        value = {'shapes': [component_to_upstream(c) for c in demand['shape']['components']],
                 'copies': demand['quantity'], 'profit': demand['profit'], 'allowed_rotations': []}
        for mirrored in ([False, True] if demand['can_mirror'] else [False]):
            value['allowed_rotations'].extend(dict(r, mirror=mirrored) for r in demand['allowed_rotations'])
        items.append(value)
    return {'objective': objective, 'parameters': {'item_item_minimum_spacing': options['item_spacing']},
            'bin_types': stocks, 'item_types': items}


def point_segment_distance(p, edge):
    dx, dy = edge.end[0] - edge.start[0], edge.end[1] - edge.start[1]
    length = dx * dx + dy * dy
    require(length > 0, 'degenerate line segment')
    t = max(0, min(1, ((p[0] - edge.start[0]) * dx + (p[1] - edge.start[1]) * dy) / length))
    return distance(p, (edge.start[0] + t * dx, edge.start[1] + t * dy))


def point_arc_distance(p, edge):
    angle = math.atan2(p[1] - edge.center[1], p[0] - edge.center[0])
    if allowed_angle(edge, angle):
        return abs(distance(p, edge.center) - distance(edge.start, edge.center))
    return min(distance(p, edge.start), distance(p, edge.end))


def intersections(first, second):
    if first.center is None and second.center is None:
        ax, ay = first.end[0] - first.start[0], first.end[1] - first.start[1]
        bx, by = second.end[0] - second.start[0], second.end[1] - second.start[1]
        determinant = ax * by - ay * bx
        if determinant == 0:
            return []  # Collinear endpoint X values are already arrangement events.
        dx, dy = second.start[0] - first.start[0], second.start[1] - first.start[1]
        t, u = (dx * by - dy * bx) / determinant, (dx * ay - dy * ax) / determinant
        return [(first.start[0] + t * ax, first.start[1] + t * ay)] if 0 <= t <= 1 and 0 <= u <= 1 else []
    if first.center is None or second.center is None:
        line, curved = (first, second) if first.center is None else (second, first)
        dx, dy = line.end[0] - line.start[0], line.end[1] - line.start[1]
        x, y = line.start[0] - curved.center[0], line.start[1] - curved.center[1]
        a, b = dx * dx + dy * dy, 2 * (x * dx + y * dy)
        discriminant = b * b - 4 * a * (x * x + y * y - distance(curved.start, curved.center) ** 2)
        if discriminant < 0 or a == 0:
            return []
        result = []
        for t in [(-b - math.sqrt(discriminant)) / (2 * a), (-b + math.sqrt(discriminant)) / (2 * a)]:
            if 0 <= t <= 1:
                p = line.start[0] + t * dx, line.start[1] + t * dy
                if allowed_angle(curved, math.atan2(p[1] - curved.center[1], p[0] - curved.center[0])):
                    result.append(p)
        return result
    r1, r2 = distance(first.start, first.center), distance(second.start, second.center)
    separation = distance(first.center, second.center)
    if separation == 0 or separation > r1 + r2 or separation < abs(r1 - r2):
        return []
    along = (r1 * r1 - r2 * r2 + separation * separation) / (2 * separation)
    height = math.sqrt(max(0, r1 * r1 - along * along))
    ux, uy = ((second.center[i] - first.center[i]) / separation for i in (0, 1))
    points = [(first.center[0] + along * ux - sign * height * uy,
               first.center[1] + along * uy + sign * height * ux) for sign in (-1, 1)]
    return [p for p in points if all(allowed_angle(e, math.atan2(p[1] - e.center[1], p[0] - e.center[0]))
                                   for e in (first, second))]


def primitive_distance(first, second):
    if intersections(first, second):
        return 0
    point_distance = lambda p, e: point_segment_distance(p, e) if e.center is None else point_arc_distance(p, e)
    result = min(point_distance(first.start, second), point_distance(first.end, second),
                 point_distance(second.start, first), point_distance(second.end, first))
    if first.center is None and second.center is None:
        return result
    if first.center is None or second.center is None:
        line, curved = (first, second) if first.center is None else (second, first)
        dx, dy = line.end[0] - line.start[0], line.end[1] - line.start[1]
        length = math.hypot(dx, dy)
        radius = distance(curved.start, curved.center)
        for sign in (-1, 1):
            p = curved.center[0] - sign * radius * dy / length, curved.center[1] + sign * radius * dx / length
            if allowed_angle(curved, math.atan2(p[1] - curved.center[1], p[0] - curved.center[0])):
                result = min(result, point_segment_distance(p, line))
        return result
    separation = distance(first.center, second.center)
    if separation > 0:
        direction = tuple((second.center[i] - first.center[i]) / separation for i in (0, 1))
        for sign1 in (-1, 1):
            a = tuple(first.center[i] + sign1 * distance(first.start, first.center) * direction[i] for i in (0, 1))
            for sign2 in (-1, 1):
                b = tuple(second.center[i] + sign2 * distance(second.start, second.center) * direction[i] for i in (0, 1))
                if all(allowed_angle(e, math.atan2(p[1] - e.center[1], p[0] - e.center[0]))
                       for e, p in [(first, a), (second, b)]):
                    result = min(result, distance(a, b))
    return result


def intervals(geometry, x):
    result = []
    for component in geometry:
        crossings = []
        for ring in component:
            for edge in ring:
                if edge.center is None:
                    if min(edge.start[0], edge.end[0]) < x < max(edge.start[0], edge.end[0]):
                        crossings.append(edge.start[1] + (x - edge.start[0]) *
                                         (edge.end[1] - edge.start[1]) / (edge.end[0] - edge.start[0]))
                else:
                    radius = distance(edge.start, edge.center)
                    radial = radius * radius - (x - edge.center[0]) ** 2
                    if radial > 0:
                        for y in [edge.center[1] - math.sqrt(radial), edge.center[1] + math.sqrt(radial)]:
                            if allowed_angle(edge, math.atan2(y - edge.center[1], x - edge.center[0])):
                                crossings.append(y)
        crossings.sort()
        require(len(crossings) % 2 == 0, 'uncertain vertical topology')
        result.extend(zip(crossings[::2], crossings[1::2]))
    merged = []
    for a, b in sorted(result):
        if merged and a <= merged[-1][1]:
            merged[-1] = merged[-1][0], max(b, merged[-1][1])
        else:
            merged.append((a, b))
    return merged


def tolerance(first, second):
    a, b = bounds(first), bounds(second)
    return max(1e-7, 1e-9 * max(1, *(abs(v) for v in a + b)))


def relation(first, second, contained=False):
    """Check every arrangement slab; no raster or chord approximation is used."""
    first_edges, second_edges = edges(first), edges(second)
    events = {p[0] for e in first_edges + second_edges for p in extrema(e)}
    events.update(p[0] for a in first_edges for b in second_edges for p in intersections(a, b))
    events = sorted(events)
    epsilon = tolerance(first, second)
    for left, right in zip(events, events[1:]):
        if right - left <= epsilon:
            continue
        a, b = intervals(first, (left + right) / 2), intervals(second, (left + right) / 2)
        if contained:
            if any(not any(c - epsilon <= start and end <= d + epsilon for c, d in b) for start, end in a):
                return False
        elif any(min(end, d) - max(start, c) > epsilon for start, end in a for c, d in b):
            return True
    return contained


def boundary_distance(first, second):
    return min(primitive_distance(a, b) for a in edges(first) for b in edges(second))


def validate_layouts(case, layouts):
    """Independently reconstruct source transforms, topology, spacing and counts."""
    problem, options = case['problem'], case['options']
    stocks, demands = problem['bins'], problem['demands']
    sources = [shape_geometry(d['shape']) for d in demands]
    counts, inventory = [0] * len(demands), [0] * len(stocks)
    total_quantity = sum(d['quantity'] for d in demands)
    require(isinstance(layouts, list) and len(layouts) <= total_quantity, 'invalid layout count')
    maximum_width, maximum_height, minimum_x, minimum_y = 0, 0, math.inf, math.inf
    maximum_x, maximum_y, material_area = -math.inf, -math.inf, 0
    layout_measures = []
    for raw in layouts:
        source = integer(raw.get('bin_index'), 'bin source')
        require(source < len(stocks), 'unknown bin source')
        stock = stocks[source]
        copies = integer(raw.get('copies', 1), 'bin copies', 1)
        require(copies <= total_quantity, 'excessive bin copies')
        inventory[source] += copies
        require(stock['quantity'] is None or inventory[source] <= stock['quantity'], 'stock inventory exceeded')
        bin_geometry = shape_geometry(stock['shape'])
        defects = [shape_geometry(d) for d in stock['defects']]
        origin = bounds(bin_geometry)[:2]
        placed, item_area, width, height = [], 0, 0, 0
        local_min_x, local_min_y, local_max_x, local_max_y = math.inf, math.inf, -math.inf, -math.inf
        require(isinstance(raw.get('placements'), list) and raw['placements'], 'empty bin placement list')
        require(len(raw['placements']) <= total_quantity, 'excessive placement count')
        for placement in raw['placements']:
            index = integer(placement.get('demand_index'), 'demand source')
            require(index < len(demands), 'unknown demand source')
            demand = demands[index]
            x, y, angle = [numeric(placement.get(k), k) for k in ('x', 'y', 'rotation_degrees')]
            mirrored = placement.get('mirrored')
            require(type(mirrored) is bool and (not mirrored or demand['can_mirror']), 'disallowed mirror')
            require(any(r['start'] <= angle <= r['end'] for r in demand['allowed_rotations']), 'disallowed rotation')
            geometry = transform(sources[index], angle, mirrored, x, y)
            require(relation(geometry, bin_geometry, contained=True), 'item outside usable bin')
            clearance = max(stock['spacing'], options.get('bin_spacing', 0))
            if clearance > 0:
                require(boundary_distance(geometry, bin_geometry) + tolerance(geometry, bin_geometry) >= clearance,
                        'bin boundary clearance violated')
            for other, spacing, label in ([(d, options['defect_spacing'], 'defect') for d in defects]
                                           + [(p, options['item_spacing'], 'item') for p in placed]):
                require(not relation(geometry, other), label + ' material overlap')
                if spacing > 0:
                    require(boundary_distance(geometry, other) + tolerance(geometry, other) >= spacing,
                            label + ' clearance violated')
            placed.append(geometry)
            counts[index] += copies
            require(counts[index] <= demand['quantity'], 'duplicated source demand')
            item_area += area(sources[index])
            box = bounds(geometry)
            minimum_x, minimum_y = min(minimum_x, box[0]), min(minimum_y, box[1])
            maximum_x, maximum_y = max(maximum_x, box[2]), max(maximum_y, box[3])
            local_min_x, local_min_y = min(local_min_x, box[0]), min(local_min_y, box[1])
            local_max_x, local_max_y = max(local_max_x, box[2]), max(local_max_y, box[3])
            width, height = max(width, box[2] - origin[0]), max(height, box[3] - origin[1])
        layout_measures.append({'used_area': item_area, 'used_width': width, 'used_height': height,
                                'x_min': local_min_x, 'y_min': local_min_y,
                                'x_max': local_max_x, 'y_max': local_max_y})
        material_area += item_area * copies
        maximum_width, maximum_height = max(maximum_width, width), max(maximum_height, height)
    if options['objective'].startswith('open_dimension_'):
        require(sum(inventory) <= 1, 'open objective exceeds one maximum bin')
    unplaced = [d['quantity'] - n for d, n in zip(demands, counts)]
    return {'bin_count': sum(inventory), 'placed_count': sum(counts), 'unplaced_quantities': unplaced,
            'source_counts': counts, 'inventory_counts': inventory, 'feasible': not any(unplaced),
            'used_area': material_area,
            'cost': sum(n * b['cost'] for n, b in zip(inventory, stocks)),
            'profit': sum(n * d['profit'] for n, d in zip(counts, demands)),
            'bin_area': sum(n * area(shape_geometry(b['shape'])) for n, b in zip(inventory, stocks)),
            'used_width': maximum_width, 'used_height': maximum_height,
            'x_min': minimum_x if layouts else 0, 'y_min': minimum_y if layouts else 0,
            'x_max': maximum_x if layouts else 0, 'y_max': maximum_y if layouts else 0,
            'layout_measures': layout_measures}


def certificate_ring(elements):
    require(isinstance(elements, list) and 1 <= len(elements) <= 2048, 'malformed certificate contour')
    result = []
    for element in elements:
        start = numeric(element.get('xs'), 'xs'), numeric(element.get('ys'), 'ys')
        end = numeric(element.get('xe'), 'xe'), numeric(element.get('ye'), 'ye')
        if element.get('type') == 'LineSegment':
            require(start != end, 'degenerate certificate line')
            result.append(Edge(start, end, None, 0))
        else:
            require(element.get('type') == 'CircularArc', 'unknown certificate element')
            orientation = element.get('orientation')
            require(orientation in ('Clockwise', 'Anticlockwise', 'Full'), 'unknown certificate arc direction')
            require(orientation != 'Full' or start == end, 'malformed full certificate arc')
            center = numeric(element.get('xc'), 'xc'), numeric(element.get('yc'), 'yc')
            result.append(arc(start, end, center, orientation == 'Clockwise'))
    epsilon = max(1e-7, 1e-9 * max(1, *(abs(v) for e in result for p in extrema(e) for v in p)))
    require(all(distance(a.end, b.start) <= epsilon for a, b in zip(result, result[1:] + result[:1])),
            'disconnected certificate contour')
    return result


def edge_middle(edge):
    initial = math.atan2(edge.start[1] - edge.center[1], edge.start[0] - edge.center[0])
    radius = distance(edge.start, edge.center)
    return (edge.center[0] + radius * math.cos(initial + edge.delta / 2),
            edge.center[1] + radius * math.sin(initial + edge.delta / 2))


def equivalent_ring(actual, expected):
    require(len(actual) == len(expected), 'certificate changed contour element count')
    epsilon = max(1e-7, 1e-9 * max(1, *(abs(v) for e in expected for p in extrema(e) for v in p)))
    used = set()
    for first in actual:
        found = None
        for index, second in enumerate(expected):
            if index in used or (first.center is None) != (second.center is None):
                continue
            if first.center is not None and second.center is not None:
                if (distance(first.center, second.center) > epsilon
                        or abs(distance(first.start, first.center) - distance(second.start, second.center)) > epsilon
                        or abs(abs(first.delta) - abs(second.delta)) > 1e-8):
                    continue
                if abs(abs(first.delta) - math.tau) < 1e-8:
                    found = index
                    break
                if distance(edge_middle(first), edge_middle(second)) > epsilon:
                    continue
            endpoints = ((distance(first.start, second.start) <= epsilon and distance(first.end, second.end) <= epsilon)
                         or (distance(first.start, second.end) <= epsilon and distance(first.end, second.start) <= epsilon))
            if endpoints:
                found = index
                break
        require(found is not None, 'certificate contour disagrees with original source transform')
        used.add(found)


def validate_certificate(case, certificate, payload):
    require(certificate is None or isinstance(certificate, dict), 'malformed upstream certificate')
    raw_bins = [] if certificate is None else certificate.get('bins')
    require(isinstance(raw_bins, list), 'missing upstream bins')
    problem = case['problem']
    layouts = []
    for raw in raw_bins:
        source = integer(raw.get('id'), 'certificate bin id')
        require(source < len(problem['bins']), 'unknown upstream stock')
        stock = problem['bins'][source]
        placements = []
        require(isinstance(raw.get('items'), list), 'missing certificate items')
        for item in raw['items']:
            index = integer(item.get('id'), 'certificate item id')
            require(index < len(problem['demands']), 'unknown upstream item')
            placement = {'demand_index': index, 'x': item.get('x'), 'y': item.get('y'),
                         'rotation_degrees': item.get('angle'), 'mirrored': item.get('mirror')}
            angle, x, y = [numeric(placement[k], k) for k in ('rotation_degrees', 'x', 'y')]
            require(type(placement['mirrored']) is bool, 'missing certificate mirror')
            geometry = transform(shape_geometry(problem['demands'][index]['shape']), angle,
                                 placement['mirrored'], x, y)
            baked = item.get('item_shapes')
            require(isinstance(baked, list) and len(baked) == len(geometry), 'changed/missing rigid components')
            for actual, component in zip(baked, geometry):
                equivalent_ring(certificate_ring(actual.get('shape')), component[0])
                holes = actual.get('holes', [])
                require(isinstance(holes, list) and len(holes) == len(component) - 1, 'changed certificate holes')
                for hole, expected in zip(holes, component[1:]):
                    equivalent_ring(certificate_ring(hole), expected)
            placements.append(placement)
        bin_geometry = shape_geometry(stock['shape'])
        require(len(bin_geometry) == 1, 'unsupported upstream bin components')
        actual_bin = certificate_ring(raw.get('shape'))
        if case['options']['objective'].startswith('open_dimension_'):
            # Fixed comparison open cases use rectangular source stock. The
            # reference writes a cropped outline; compare the exact crop here.
            source_box = bounds(bin_geometry)
            expected_box = list(source_box)
            item_boxes = [bounds(transform(shape_geometry(problem['demands'][p['demand_index']]['shape']),
                                          p['rotation_degrees'], p['mirrored'], p['x'], p['y'])) for p in placements]
            require(item_boxes, 'empty open certificate')
            axis = 2 if case['options']['objective'] == 'open_dimension_x' else 3
            expected_box[axis] = max(b[axis] for b in item_boxes)
            a, b, c, d = expected_box
            expected = contour_edges({'kind': 'polygon', 'vertices': [point_json(p) for p in
                                      [(a, b), (c, b), (c, d), (a, d)]]})
            require(all(e.center is None for e in bin_geometry[0][0]) and len(bin_geometry[0][0]) == 4,
                    'open comparison requires rectangular source stock')
            equivalent_ring(actual_bin, expected)
        else:
            equivalent_ring(actual_bin, bin_geometry[0][0])
        defects = raw.get('defects', [])
        require(isinstance(defects, list) and len(defects) == len(stock['defects']), 'changed certificate defects')
        for actual, source_defect in zip(defects, stock['defects']):
            geometry = shape_geometry(source_defect)
            require(len(geometry) == 1, 'unsupported defect components')
            equivalent_ring(certificate_ring(actual.get('shape')), geometry[0][0])
            holes = actual.get('holes', [])
            require(len(holes) == len(geometry[0]) - 1, 'changed defect holes')
            for hole, expected in zip(holes, geometry[0][1:]):
                equivalent_ring(certificate_ring(hole), expected)
        layouts.append({'bin_index': source, 'copies': integer(raw.get('copies'), 'certificate copies', 1),
                        'placements': placements})
    checked = validate_layouts(case, layouts)
    require(isinstance(payload, dict) and isinstance(payload.get('Output'), dict), 'missing upstream output')
    output = payload['Output']
    summary = output.get('Solution')
    require(isinstance(summary, dict), 'missing upstream solution summary')
    for key, expected in [('NumberOfItems', checked['placed_count']), ('NumberOfBins', checked['bin_count']),
                          ('ItemArea', checked['used_area']), ('ItemProfit', checked['profit']),
                          ('BinArea', checked['bin_area']), ('BinCost', checked['cost']),
                          ('FullWaste', checked['bin_area'] - checked['used_area'])]:
        close_number(summary.get(key), expected, key)
    # Pinned reference extrema describe the last bin group, not all bins. Its
    # untouched infinity sentinels serialize as null when no material is placed.
    for key, normalized in [('XMin', 'x_min'), ('YMin', 'y_min'), ('XMax', 'x_max'), ('YMax', 'y_max')]:
        if checked['layout_measures']:
            close_number(summary.get(key), checked['layout_measures'][-1][normalized], key)
        else:
            require(key in summary and summary[key] is None, 'empty upstream extrema must be null')
    require(numeric(output.get('Time'), 'Time') >= 0, 'negative solve time')
    return checked


def validate_rust(case, solution, options):
    require(isinstance(solution, dict), 'missing Rust solution')
    require(solution.get('algorithm') == 'native_irregular_contact', 'unexpected Rust algorithm')
    require(isinstance(solution.get('options'), dict), 'missing Rust controls')
    for key, expected in options.items():
        require(solution['options'].get(key) == expected, 'Rust changed requested controls: ' + key)
    layouts = solution.get('layouts')
    require(isinstance(layouts, list) and all(raw.get('copies', 1) == 1 for raw in layouts),
            'Rust layouts cannot invent a copy multiplier')
    checked = validate_layouts(case, layouts)
    unplaced = solution.get('unplaced_quantities')
    require(isinstance(unplaced, list) and all(type(n) is int and n >= 0 for n in unplaced)
            and unplaced == checked['unplaced_quantities'], 'incorrect Rust shortfall accounting')
    require(type(solution.get('feasible')) is bool and solution['feasible'] == checked['feasible'], 'incorrect Rust feasibility')
    require(type(solution.get('search_truncated')) is bool, 'missing Rust search status')
    require(integer(solution.get('explored_states'), 'explored states') <= options['max_states'], 'Rust exceeded work budget')
    require(integer(solution.get('construction_probes'), 'construction probes') <= 32768, 'Rust exceeded construction budget')
    close_number(solution.get('total_cost'), checked['cost'], 'total_cost')
    close_number(solution.get('total_profit'), checked['profit'], 'total_profit')
    for raw, measure in zip(layouts, checked['layout_measures']):
        close_number(raw.get('used_area'), measure['used_area'], 'layout area')
        padding = 0
        for p in raw['placements']:
            shape = case['problem']['demands'][p['demand_index']]['shape']
            if any(e.center is not None for e in edges(shape_geometry(shape))):
                padding = 2.5 * options['curve_tolerance']
        for key in ('used_width', 'used_height'):
            value = numeric(raw.get(key), key)
            require(measure[key] - 1e-7 <= value <= measure[key] + padding + 1e-7,
                    'Rust extents disagree with original/conservative geometry')
    checked['search_truncated'] = solution['search_truncated']
    checked['explored_states'] = solution['explored_states']
    checked['construction_probes'] = solution['construction_probes']
    return checked


PUBLIC_FIXTURES = ('circle', 'circular_bin', 'rotations', 'mirror',
                   'feasibility_squares_triangles', 'polygon_with_hole', 'bin_packing_squares',
                   'knapsack_item_item_minimum_spacing', 'knapsack_item_defect_minimum_spacing',
                   'open_dimension_x_mixed_shapes')


def rectangle(width, height, x=0, y=0):
    return {'type': 'rectangle', 'x': x, 'y': y, 'width': width, 'height': height}


def polygon(vertices):
    return {'type': 'polygon', 'vertices': [point_json(p) for p in vertices]}


def targeted_cases():
    l_shape = polygon([(0, 0), (2, 0), (2, 1), (1, 1), (1, 2), (0, 2)])
    mirrored_l = polygon([(0, 0), (2, 0), (2, 2), (1, 2), (1, 1), (0, 1)])
    semicircle = {'type': 'general', 'elements': [
        {'type': 'LineSegment', 'start': {'x': 0, 'y': 0}, 'end': {'x': 4, 'y': 0}},
        {'type': 'CircularArc', 'start': {'x': 4, 'y': 0}, 'end': {'x': 0, 'y': 0},
         'center': {'x': 2, 'y': 0}, 'orientation': 'Anticlockwise'}]}
    definitions = [
        ('l-interlocking', 'feasibility', [rectangle(2, 3)],
         [dict(l_shape, copies=2, allowed_rotations=[{'start': 0, 'end': 0}, {'start': 180, 'end': 180}])], {}),
        ('item-in-hole', 'feasibility', [rectangle(4, 4)],
         [dict(rectangle(4, 4), holes=[rectangle(2, 2, 1, 1)]), rectangle(2, 2)], {}),
        ('rigid-components', 'feasibility', [rectangle(3, 2)],
         [{'shapes': [rectangle(1, 2), rectangle(1, 2, 2, 0)]}, rectangle(1, 2)], {}),
        ('circular-arc', 'feasibility', [rectangle(5, 3)], [semicircle], {}),
        ('circle-tangencies', 'knapsack', [{'type': 'circle', 'radius': 5}],
         [{'type': 'circle', 'radius': 2, 'copies': 2}], {}),
        ('mirrored-concavity', 'feasibility', [mirrored_l], [dict(l_shape, allow_mirroring=True)], {}),
        ('rotation-interval', 'feasibility', [rectangle(3, 1)],
         [dict(rectangle(1, 3), allowed_rotations=[{'start': 89, 'end': 91}])], {}),
        ('item-spacing', 'feasibility', [rectangle(5, 2)],
         [dict(rectangle(2, 2), copies=2)], {'item_item_minimum_spacing': 1}),
        ('defect-spacing', 'feasibility', [dict(rectangle(7, 2), defects=[
            dict(rectangle(1, 2, 3, 0), item_defect_minimum_spacing=0.5)])],
         [dict(rectangle(2, 2), copies=2)], {}),
        ('cost-objective', 'variable-sized-bin-packing',
         [dict(rectangle(4, 4), copies=2, cost=1), dict(rectangle(8, 4), copies=1, cost=10)],
         [dict(rectangle(4, 4), copies=2)], {}),
        ('open-x', 'open-dimension-x', [rectangle(10, 2)], [dict(rectangle(2, 2), copies=2)], {}),
        ('open-y', 'open-dimension-y', [rectangle(2, 10)], [dict(rectangle(2, 2), copies=2)], {}),
        ('fractional-contact', 'feasibility', [rectangle(2.5, 1.25)],
         [dict(rectangle(1.25, 1.25), copies=2)], {}),
        ('curved-hole', 'feasibility', [rectangle(5, 5)],
         [{'type': 'circle', 'radius': 2, 'holes': [{'type': 'circle', 'radius': 1}]},
          {'type': 'circle', 'radius': 0.8}], {}),
    ]
    result = []
    for name, objective, stocks, items, parameters in definitions:
        payload = {'objective': objective, 'parameters': parameters,
                   'bin_types': [dict(b, cost=b.get('cost', 1), copies=b.get('copies', 1)) for b in stocks],
                   'item_types': [dict(i, profit=i.get('profit', 1)) for i in items]}
        case = normalize_upstream(payload, 'targeted-' + name)
        case['provenance'] = {'kind': 'fixed_targeted_case', 'definition_sha256': canonical_hash(payload),
                              'development_feedback': 'Feature regression, selected before reference outcome inspection'}
        result.append(case)
    return result


def public_cases(source):
    result = []
    for name in PUBLIC_FIXTURES:
        path = source / 'data' / 'irregular' / 'tests' / (name + '.json')
        case = normalize_upstream(json.loads(path.read_text()), 'public-' + name)
        case['category'] = 'public_fixture'
        case['provenance'] = {'kind': 'pinned_upstream_test', 'revision': UPSTREAM_REVISION,
                              'file': str(path.relative_to(source)), 'sha256': file_hash(path),
                              'normalization': 'Preserve original geometry, holes, components, angles, mirrors, copies, spacing, and objective; material-area cost/profit and one-copy stock defaults made explicit; source indices become display labels'}
        result.append(case)
    return result


def canonical_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


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
              ROOT / 'crates/bin-packing/examples/packingsolver_compare.rs',
              ROOT / 'scripts/compare-irregular-packingsolver.py',
              ROOT / 'scripts/tests/test_compare_irregular.py']
    hashes = {str(p.relative_to(ROOT)): file_hash(p) for p in paths}
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
    (folder / 'command.json').write_text(json.dumps(command, indent=2) + '\n')
    result.check_returncode()
    return result, wall


def run_rust(case, binary, folder, args, environment):
    options = dict(case['options'], max_states=args.max_states, beam_width=8,
                   max_candidates_per_item=128, rotation_step_degrees=15,
                   max_orientations=64, curve_tolerance=0.01)
    request = {'dimension': 'irregular', 'problem': case['problem'], 'options': options,
               'warmups': args.warmups, 'repeats': args.repeats, 'max_states': args.max_states}
    encoded = json.dumps(request, allow_nan=False)
    (folder / 'input.json').write_text(encoded + '\n')
    output, wall = run_process([str(binary)], folder, environment,
                               60 * (args.warmups + args.repeats), encoded)
    payload = json.loads(output.stdout)
    require(payload.get('release_build') is True, 'Rust harness must be a release build')
    require(payload.get('dimension') == 'irregular' and payload.get('warmups') == args.warmups
            and payload.get('repeats') == args.repeats
            and payload.get('compiled_features', {}).get('irregular') is True, 'wrong Rust harness configuration')
    variants = payload.get('variants')
    require(isinstance(variants, list) and len(variants) == 1 and variants[0].get('variant') == 'irregular',
            'missing or unexpected Rust variant')
    variant = variants[0]
    samples = variant.get('samples_ns')
    require(isinstance(samples, list) and len(samples) == args.repeats
            and all(type(v) is int and v > 0 for v in samples), 'invalid Rust timing samples')
    checked = validate_rust(case, variant.get('result'), options)
    return {'request_sha256': canonical_hash(request), 'samples_seconds': [v / 1e9 for v in samples],
            'median_internal_seconds': statistics.median(samples) / 1e9,
            'process_seconds_all_samples': wall, 'validated': checked, 'full_result': payload}


def run_upstream(case, binary, folder, args, environment):
    translated = upstream_input(case)
    input_path = folder / 'input.json'
    input_path.write_text(json.dumps(translated, indent=2, allow_nan=False) + '\n')
    rebuilt = normalize_upstream(translated)
    require(rebuilt['problem'] == case['problem'] and rebuilt['options'] == case['options'],
            'input translation failed exact canonical roundtrip')
    samples = []
    for index in range(args.warmups + args.repeats):
        sample = folder / ('warmup-' + str(index) if index < args.warmups
                           else 'sample-' + str(index - args.warmups))
        sample.mkdir()
        command = [str(binary), '--input', str(input_path), '--optimization-mode',
                   'not-anytime-sequential', '--time-limit', str(args.time_limit),
                   '--linear-programming-solver', 'highs', '--verbosity-level', '0',
                   '--only-write-at-the-end', '--output', str(sample / 'output.json'),
                   '--certificate', str(sample / 'certificate.json')]
        _, wall = run_process(command, sample, environment, args.time_limit + 30)
        payload = json.loads((sample / 'output.json').read_text())
        certificate = json.loads((sample / 'certificate.json').read_text())
        checked = validate_certificate(case, certificate, payload)
        if index >= args.warmups:
            samples.append({'process_seconds': wall, 'internal_seconds': payload['Output']['Time'],
                            'validated': checked, 'full_result': payload,
                            'certificate_sha256': file_hash(sample / 'certificate.json')})
    return {'input_sha256': canonical_hash(translated), 'samples': samples,
            'median_internal_seconds': statistics.median(s['internal_seconds'] for s in samples),
            'median_process_seconds': statistics.median(s['process_seconds'] for s in samples),
            'all_samples_same_quality': all(compare_quality(objective_score(case, samples[0]['validated']),
                                                            objective_score(case, s['validated'])) == 0 for s in samples),
            'all_samples_identical_metrics': len({canonical_hash(s['validated']) for s in samples}) == 1}


def objective_score(case, checked):
    objective = case['options']['objective']
    missing = sum(checked['unplaced_quantities'])
    if objective == 'knapsack':
        return (-checked['profit'],)
    if objective == 'total_cost':
        return missing, checked['cost']
    if objective == 'open_dimension_x':
        return missing, checked['used_width']
    if objective == 'open_dimension_y':
        return missing, checked['used_height']
    if objective == 'feasibility':
        return (missing,)
    return missing, checked['bin_count']


def compare_quality(first, second):
    for a, b in zip(first, second):
        if not math.isclose(a, b, rel_tol=1e-9, abs_tol=1e-7):
            return -1 if a < b else 1
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rust-binary', required=True, type=Path)
    parser.add_argument('--irregular-binary', required=True, type=Path)
    parser.add_argument('--packingsolver-source', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--warmups', type=int, default=1)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--time-limit', type=float, default=2)
    parser.add_argument('--max-states', type=int, default=4096)
    parser.add_argument('--case', action='append')
    parser.add_argument('--final', action='store_true', help='Mark a run after the source owner freezes the implementation')
    args = parser.parse_args()
    require(1 <= args.warmups <= 10 and 1 <= args.repeats <= 51, 'invalid sample count')
    require(math.isfinite(args.time_limit) and 0 < args.time_limit <= 60, 'invalid reference time limit')
    require(0 <= args.max_states <= 200000, 'invalid Rust improvement budget')
    scratch = Path('/Volumes/Scratch')
    require(scratch.is_mount(), 'Scratch is not mounted')
    output_path = args.output.resolve()
    require(output_path.is_relative_to(scratch) and not output_path.exists(), 'report must be new and under Scratch')
    args.packingsolver_source = args.packingsolver_source.resolve()
    require(command_output(['git', 'rev-parse', 'HEAD'], args.packingsolver_source) == UPSTREAM_REVISION,
            'reference source revision is not pinned')
    require(not command_output(['git', 'status', '--porcelain'], args.packingsolver_source), 'reference source is dirty')
    for name in ('rust_binary', 'irregular_binary'):
        binary = getattr(args, name).resolve()
        require(binary.is_file() and os.access(binary, os.X_OK), 'missing executable ' + str(binary))
        setattr(args, name, binary)
    cases = targeted_cases() + public_cases(args.packingsolver_source)
    if args.case:
        require(set(args.case) <= {c['name'] for c in cases}, 'unknown selected case')
        cases = [c for c in cases if c['name'] in args.case]
    output_path.parent.mkdir(parents=True, exist_ok=True)
    artifacts = output_path.parent / (output_path.stem + '-artifacts')
    artifacts.mkdir()
    environment = dict(os.environ, RAYON_NUM_THREADS='1', OMP_NUM_THREADS='1',
                       OPENBLAS_NUM_THREADS='1', TMPDIR=str(artifacts), PYTHONDONTWRITEBYTECODE='1')
    binaries = {'rust': args.rust_binary, 'packingsolver': args.irregular_binary}
    metadata = source_metadata()
    report = {'measurement_status': 'final_frozen_source' if args.final else 'diagnostic_not_final',
              'timestamp_utc': datetime.now(timezone.utc).isoformat(), 'source': metadata,
              'reference_revision': UPSTREAM_REVISION,
              'binaries': {k: {'path': str(v), 'sha256': file_hash(v)} for k, v in binaries.items()},
              'host': {'platform': platform.platform(), 'cpu_count': os.cpu_count(),
                       'load_average_start': list(os.getloadavg())},
              'method': {'warmups': args.warmups, 'repeats': args.repeats,
                         'upstream_mode': 'not-anytime-sequential', 'upstream_lp_solver': 'highs',
                         'upstream_time_limit_seconds': args.time_limit,
                         'rust_improvement_geometry_probes': args.max_states,
                         'rust_greedy_geometry_probe_cap': 32768,
                         'equal_effort_budgets': False, 'time_to_equal_quality_measured': False,
                         'native_repeat_determinism_verified_by_harness': True,
                         'timing_scope': 'Native solve calls include clone/validation; reference reports internal Time. Parsing, independent Python validation, serialization and process startup excluded from internal timings. Process wall times retained separately.',
                         'geometry_validation': 'Independent analytic line/circular-arc vertical arrangements and boundary distances; spatial tolerance max(1e-7, 1e-9 * absolute coordinate scale), original material shapes (no bbox or chord-only validator)',
                         'parallel_environment': {k: environment[k] for k in ('RAYON_NUM_THREADS', 'OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS')},
                         'suite_definition': '14 fixed targeted feature cases plus 10 fixed pinned upstream public tests; not representative of production or global performance',
                         'selected_case_names': [c['name'] for c in cases],
                         'selected_targeted_cases': sum(c['category'] == 'targeted' for c in cases),
                         'selected_public_cases': sum(c['category'] == 'public_fixture' for c in cases)},
              'semantic_exclusions': {
                  'open_dimension_xy': 'Reference requires fixed aspect ratio; native free-aspect XY objective differs',
                  'mirror_only_rotation': 'Reference per-angle mirrored-only permissions differ from global optional native mirroring',
                  'bin_holes_components': 'Native supports these; reference bin reader uses a single unholed outer Shape',
                  'fixed_items_quality_resources_periodic': 'Outside the matching irregular wire model and these comparisons'},
              'cases': [], 'failures': []}
    cache = args.irregular_binary.parents[2] / 'CMakeCache.txt'
    if cache.is_file():
        report['reference_build_configuration'] = [line for line in cache.read_text().splitlines()
            if line.startswith(('PACKINGSOLVER_', 'CMAKE_BUILD_TYPE:', 'CMAKE_CXX_COMPILER:',
                                'CMAKE_CXX_FLAGS_RELEASE:', 'CMAKE_HOME_DIRECTORY:'))]
    for case in cases:
        print(case['name'], flush=True)
        folder = artifacts / case['name']
        folder.mkdir()
        row = {'case': case, 'canonical_case_sha256': canonical_hash(case), 'host_load_start': list(os.getloadavg())}
        for engine, function in [('rust', run_rust), ('packingsolver', run_upstream)]:
            target = folder / engine
            target.mkdir()
            try:
                row[engine] = function(case, binaries[engine], target, args, environment)
            except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
                failure = {'case': case['name'], 'engine': engine, 'error': str(error)}
                row[engine] = {'failure': failure}
                report['failures'].append(failure)
                print('FAILED ' + engine + ': ' + str(error), file=sys.stderr, flush=True)
        if 'failure' not in row['rust'] and 'failure' not in row['packingsolver']:
            native = objective_score(case, row['rust']['validated'])
            reference = [objective_score(case, s['validated']) for s in row['packingsolver']['samples']]
            relations = [compare_quality(native, score) for score in reference]
            status = ('equal' if all(v == 0 for v in relations) else
                      'rust_better' if all(v < 0 for v in relations) else
                      'reference_better' if all(v > 0 for v in relations) else 'mixed_reference_samples')
            native_seconds = row['rust']['median_internal_seconds']
            row['comparison'] = {'quality': status, 'rust_score': native, 'reference_sample_scores': reference,
                                 'rust_median_internal_seconds': native_seconds,
                                 'reference_median_internal_seconds': row['packingsolver']['median_internal_seconds'],
                                 'reference_over_rust_internal_runtime_ratio': row['packingsolver']['median_internal_seconds'] / native_seconds,
                                 'equal_quality_in_all_samples': all(v == 0 for v in relations),
                                 'time_to_equal_quality_measured': False}
        row['host_load_end'] = list(os.getloadavg())
        report['cases'].append(row)
        output_path.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    report['source_unchanged_during_run'] = metadata['combined_source_sha256'] == source_metadata()['combined_source_sha256']
    report['binaries_unchanged_during_run'] = all(file_hash(path) == report['binaries'][name]['sha256'] for name, path in binaries.items())
    if args.final and (not report['source_unchanged_during_run'] or not report['binaries_unchanged_during_run']):
        report['failures'].append({'engine': 'provenance', 'error': 'source or executable changed during final measurements'})
    report['all_cases_validated'] = not report['failures']
    report['host']['load_average_end'] = list(os.getloadavg())
    output_path.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    print('Wrote ' + str(output_path), flush=True)
    return 0 if report['all_cases_validated'] else 1


if __name__ == '__main__':
    sys.exit(main())
