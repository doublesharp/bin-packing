const assert = require('node:assert/strict');
const {replaySawJob} = require('./test/saw-replay.cjs');

function strips(count = 4, sheets = 1) {
  const width = count * 300 + (count - 1) * 3;
  return {
    problem: {
      sheets: [{ name: 'stock', width, height: 600, kerf: 3 }],
      demands: [{ name: 'panel', width: 300, height: 600, quantity: count * sheets, can_rotate: false }],
    },
    layouts: Array.from({ length: sheets }, () => ({ width, height: 600,
      placements: Array.from({ length: count }, (_, i) => ({ name: 'panel', x: i * 303, y: 0, width: 300, height: 600, rotated: false })),
      sum_sq_usable_drop_areas: 1e30,
    })),
    stock_indices: Array(sheets).fill(0),
    demand_indices: Array.from({ length: sheets }, () => Array(count).fill(0)),
  };
}

function smoke(api) {
  const job = strips();
  const plan = api.plan2dSawCuts(job);
  replaySawJob(job,plan);
  assert.deepEqual(plan.counts, { cuts: 3, initial_setups: 1, setup_changes: 0, rotations: 0, handling: 3 });
  assert.equal(plan.schema_version, 1);
  assert.equal(plan.machine, 'table_saw');
  assert.equal(plan.objective, 'weighted_effort');
  assert.equal(plan.batch_across_sheets, false);
  assert.deepEqual(plan.effective_weights, { cut: 1, initial_setup: 5, setup_change: 5, rotation: 2, handling: 1 });
  assert.equal(plan.effort_score, 11);
  assert.equal(plan.workpieces.filter(p => p.finished_part !== null).length, 4);
  for (const piece of plan.workpieces) {
    assert.ok(piece.parent === null || typeof piece.parent.sheet_index === 'number');
    assert.ok(piece.created_by === null || Number.isInteger(piece.created_by));
    assert.equal(typeof piece.usable_offcut, 'boolean');
    if (piece.finished_part !== null) {
      assert.equal(piece.finished_part.sheet_index, 0);
      assert.equal(typeof piece.finished_part.placement_index, 'number');
    }
  }
  const kinds = new Set();
  for (const [id, operation] of plan.operations.entries()) {
    assert.equal(operation.id, id);
    assert.equal(operation.workpiece.sheet_index, 0);
    assert.ok(operation.depends_on === null || operation.depends_on < id);
    kinds.add(operation.action.kind);
    if (operation.action.kind === 'cut') {
      const cut = operation.action;
      assert.equal(cut.axis, 'vertical');
      assert.equal(cut.setting, 300);
      assert.equal(cut.reference_edge, 'left');
      assert.equal(cut.kerf, 3);
      assert.equal(cut.kerf_region.width, 3);
      assert.equal(cut.segment.to_y - cut.segment.from_y, 600);
      assert.equal(cut.children.length, 2);
    }
  }
  assert.deepEqual([...kinds].sort(), ['cut', 'handle', 'setup']);
  assert.equal(plan.search.limits.max_states, 4000);
  assert.equal(typeof plan.search.explored_states, 'number');
  assert.equal(typeof plan.search.baseline_states, 'number');
  assert.equal(typeof plan.search.budget_exhausted, 'boolean');
  assert.equal(typeof plan.search.beam_pruned, 'boolean');
  assert.ok(Number.isSafeInteger(plan.search.dominated_states));
  assert.ok(plan.search.dominated_states >= 0);
  assert.ok(Number.isSafeInteger(plan.search.peak_dominance_units));
  assert.ok(plan.search.peak_dominance_units >= 0 && plan.search.peak_dominance_units <= 65536);
  assert.equal(typeof plan.search.dominance_table_saturated, 'boolean');
  assert.ok(['heuristic', 'optimal_within_candidate_model'].includes(plan.search.optimality));
  assert.equal(typeof plan.search.baseline_effort_score, 'number');
  assert.equal(plan.search.baseline_counts.cuts, 3);
  assert.deepEqual(plan.sheet_views[0], { sheet_index_in_solution: 0, stock_index: 0, operation_ids: plan.operations.map(op => op.id) });
  const baseline=api.plan2dSawCuts(job,{search:{max_states:0}});
  replaySawJob(job,baseline,{search:{max_states:0}});
  assert.equal(baseline.search.explored_states,0);
  assert.equal(baseline.search.budget_exhausted,true);
  assert.equal(baseline.search.optimality,'heuristic');
  assert.throws(() => api.plan2dSawCuts(job, { machine: 'cnc_router' }));
  assert.throws(() => api.plan2dSawCuts(job, { weights: { cut: -1 } }));
  assert.throws(() => api.plan2dSawCuts({ ...job, stock_indices: [5] }));
  // Real object solve -> plan, with large metrics serialized in exponent notation.
  const problem = { sheets: [{ name: 'large', width: 1000000, height: 900000 }],
    demands: [{ name: 'p', width: 12345, height: 54321, quantity: 1, can_rotate: false }] };
  const solution = api.solve2d(problem, { algorithm: 'guillotine' });
  const largeJob={ problem, layouts: solution.layouts, stock_indices: [0], demand_indices: [[0]] };
  const large = api.plan2dSawCuts(largeJob);
  replaySawJob(largeJob,large);
  const options={weights:{cut:3,initial_setup:7,setup_change:11,rotation:13,handling:17},min_usable_side:10};
  replaySawJob(largeJob,api.plan2dSawCuts(largeJob,options),options);
  assert.equal(large.workpieces.filter(p => p.finished_part !== null).length, 1);
  assert.ok(large.operations.some(op => op.action.kind === 'rotate'));
  assert.ok(large.operations.some(op => op.action.kind === 'setup' && op.action.setup.kind === 'crosscut_stop'));
}
module.exports = { smoke, strips };

function mutationSmoke(api) {
  const job=strips();
  const plan=api.plan2dSawCuts(job);
  const mutations=[
    ['parent',p=>p.workpieces.forEach(w=>{w.parent=null;}),/wrong parent/],
    ['dependency',p=>p.operations.forEach(op=>{op.depends_on=null;}),/creating dependency/],
    ['segment',p=>p.operations.forEach(op=>{if(op.action.kind==='cut'){op.action.segment.from_x=0;op.action.segment.to_x=0;}}),/diagram segment/],
    ['part identity',p=>p.workpieces.forEach(w=>{if(w.finished_part)w.finished_part.placement_index=0;}),/finished part identity/],
    ['blade',p=>p.operations.forEach(op=>{if(op.action.kind==='cut')op.action.kerf=100;}),/blade kerf/],
    ['accessory',p=>p.operations.forEach(op=>{if(op.action.kind==='setup')op.action.setup.kind='panel_stop';}),/physical setup/],
    ['views',p=>{p.sheet_views=[];},/sheet views/],
  ];
  for (const [name,mutate,error] of mutations) {
    const broken=structuredClone(plan);mutate(broken);
    assert.throws(()=>replaySawJob(job,broken),error,name);
  }
  const rects=[[0,0,5,9],[5,0,8,8],[13,0,2,7],[13,7,2,2]];
  const demands=rects.map(([, ,width,height],i)=>({name:`p${i}`,width,height,quantity:2,can_rotate:false}));
  const placements=rects.map(([x,y,width,height],i)=>({name:`p${i}`,x,y,width,height,rotated:false}));
  const batchJob={problem:{sheets:[{name:'stock',width:23,height:19}],demands},
    layouts:[{width:23,height:19,placements},{width:23,height:19,placements}],
    stock_indices:[0,0],demand_indices:[[0,1,2,3],[0,1,2,3]]};
  const options={machine:'panel_saw',batch_across_sheets:true,search:{max_states:30000,beam_width:64}};
  const batched=api.plan2dSawCuts(batchJob,options);
  replaySawJob(batchJob,batched,options);
  batched.batch_across_sheets=false;
  assert.throws(()=>replaySawJob(batchJob,batched,{...options,batch_across_sheets:false}),/sheet order/);

}
module.exports.mutationSmoke=mutationSmoke;
