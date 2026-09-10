// Build native addon first. Produces repeatable before/after schedule evidence.
const api = require('../node/wrapper.js');
function job(parts, sheets = 1) {
  return { problem: { sheets: [{ name: 'plywood', width: 23, height: 19, kerf: 0 }],
    demands: parts.map(([, , width, height]) => ({ name: 'part', width, height, quantity: sheets, can_rotate: false })) },
    layouts: Array.from({ length: sheets }, () => ({ width: 23, height: 19,
      placements: parts.map(([x,y,width,height]) => ({ name: 'part', x, y, width, height, rotated: false })) })),
    stock_indices: Array(sheets).fill(0), demand_indices: Array.from({ length: sheets }, () => parts.map((_,i) => i)) };
}
function view(plan) {
  return { counts: plan.counts, effort_score: plan.effort_score, operations: plan.operations,
    passes: plan.operations.filter(op => op.action.kind === 'cut').map(op => ({
      operation_id: op.id, sheet: op.workpiece.sheet_index, bounds: op.workpiece.bounds,
      axis: op.action.axis, edge: op.action.reference_edge, setting: op.action.setting,
      segment: op.action.segment })),
    setups: plan.operations.filter(op => op.action.kind === 'setup').map(op => ({ id: op.id, sheet: op.workpiece.sheet_index, ...op.action })) };
}
const options = { machine: 'panel_saw', search: { max_states: 30000, beam_width: 64 } };
const parts = [[0,0,5,9],[5,0,8,8],[13,0,2,7],[13,7,2,2]];
const single = job(parts), multiple = job(parts,2);
const tradeoff = job([[0,0,10,7],[10,0,7,8],[10,8,7,5],[10,13,4,6]]);
const problem = { sheets: [{name:'sheet',width:1200,height:600,kerf:0}],demands:[{name:'panel',width:300,height:600,quantity:4,can_rotate:false}] };
const solution = api.solve2d(problem,{algorithm:'guillotine'});
const result = {
  strips_legacy: api.plan2dCuts(solution),
  strips_physical: view(api.plan2dSawCuts({problem,layouts:solution.layouts,stock_indices:[0],demand_indices:[[0,0,0,0]]})),
  weighted_rotation: view(api.plan2dSawCuts(single,{...options,weights:{setup_change:0,rotation:100}})),
  weighted_setups: view(api.plan2dSawCuts(single,{...options,weights:{setup_change:100,rotation:0}})),
  cut_first: view(api.plan2dSawCuts(tradeoff,{...options,objective:'fewest_cuts'})),
  setup_first: view(api.plan2dSawCuts(tradeoff,{...options,objective:'fewest_setup_changes'})),
  sheets_sequential: view(api.plan2dSawCuts(multiple,options)),
  sheets_batched: view(api.plan2dSawCuts(multiple,{...options,batch_across_sheets:true})),
};
console.log(JSON.stringify(result,null,2));
