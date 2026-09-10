const checkSawPlan = typeof require === 'function' ? require('./saw-replay.cjs').replaySawJob : replaySawJob;

// Shared browser/native workload and measurement code; no runtime dependencies.
function grid(columns, rows, sheets = 1) {
  const width = columns * 145 + (columns - 1) * 3;
  const height = rows * 97 + (rows - 1) * 3;
  return { problem: {
    sheets: [{ name: 'plywood', width, height, kerf: 3 }],
    demands: [{ name: 'panel', width: 145, height: 97, quantity: columns * rows * sheets, can_rotate: false }],
  }, layouts: Array.from({ length: sheets }, () => ({ width, height,
    placements: Array.from({ length: columns * rows }, (_, i) => ({ name: 'panel',
      x: i % columns * 148, y: Math.floor(i / columns) * 100, width: 145, height: 97, rotated: false })) })),
    stock_indices: Array(sheets).fill(0),
    demand_indices: Array.from({ length: sheets }, () => Array(columns * rows).fill(0)) };
}
function measure(api) {
  return [[4,1,1], [8,4,1], [8,8,1], [8,8,2]].map(([columns, rows, sheets]) => {
    const job = grid(columns, rows, sheets);
    const options = { batch_across_sheets: sheets > 1 };
    api.plan2dSawCuts(job, options);
    const times = [];
    let plan;
    for (let i = 0; i < 7; i++) {
      const start = performance.now();
      plan = api.plan2dSawCuts(job, options);
      times.push(performance.now() - start);
    }
    times.sort((a,b) => a-b);
    if (plan.counts.cuts !== columns * rows * sheets - sheets) throw new Error('unexpected grid cut count');
    checkSawPlan(job,plan,options);
    // A known feasible grid witness uses at most two setting changes. Merely
    // beating a baseline could hide regressions if that baseline also degraded.
    if (plan.counts.setup_changes > 2) throw new Error('grid setup quality regressed');
    if (plan.effort_score > plan.search.baseline_effort_score) throw new Error('worse than baseline');
    return { parts: columns * rows * sheets, sheets, median_ms: +times[3].toFixed(3), max_ms: +times[6].toFixed(3),
      counts: plan.counts, effort_score: plan.effort_score, search: plan.search };
  });
}
if (typeof module !== 'undefined') module.exports = { grid, measure };

function measureStrips(api) {
  return [128,256,512].map(count=>{
    const job=grid(count,1);
    const options={search:{max_states:0}};
    api.plan2dSawCuts(job,options);
    const times=[];let plan;
    for(let i=0;i<7;i++){const start=performance.now();plan=api.plan2dSawCuts(job,options);times.push(performance.now()-start);}
    times.sort((a,b)=>a-b);checkSawPlan(job,plan,options);
    if(plan.counts.cuts!==count-1||plan.counts.setup_changes!==0||plan.search.baseline_states>count)
      throw new Error('straight strip construction quality regressed');
    return {parts:count,median_ms:+times[3].toFixed(3),max_ms:+times[6].toFixed(3),counts:plan.counts,search:plan.search};
  });
}
if(typeof module!=='undefined') module.exports.measureStrips=measureStrips;
