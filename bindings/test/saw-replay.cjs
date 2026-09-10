// Independent consumer-side replay for native, Node-WASM and browser-WASM output.
// This file has no Node dependencies so the browser benchmark can load it directly.
function replaySawJob(job, plan, options = {}) {
  const equal = (a,b,message) => { if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(message); };
  const check = (condition,message) => { if (!condition) throw new Error(message); };
  const key = id => [id.sheet_index,id.bounds.x,id.bounds.y,id.bounds.width,id.bounds.height].join(':');
  const rect = p => ({x:p.x,y:p.y,width:p.width,height:p.height});
  const box = b => [b.x,b.y,b.width,b.height];
  const weights = {cut:1,initial_setup:5,setup_change:5,rotation:2,handling:1,...options.weights};
  equal(plan.effective_weights,weights,'effective weights differ from request');
  equal(plan.machine,options.machine??'table_saw','wrong machine');
  equal(plan.objective,options.objective??'weighted_effort','wrong objective');
  equal(plan.batch_across_sheets,options.batch_across_sheets??false,'wrong batching option');
  const live = new Map(), ancestry = new Map();
  for (const [sheet_index,l] of job.layouts.entries()) {
    const id={sheet_index,bounds:{x:0,y:0,width:l.width,height:l.height}};
    live.set(key(id),0); ancestry.set(key(id),{parent:null,created_by:null});
  }
  let loaded=null, setup=null, lastSheet=0;
  const seen = new Set();
  const counts={cuts:0,initial_setups:0,setup_changes:0,rotations:0,handling:0};
  for (const [index,op] of plan.operations.entries()) {
    equal(op.id,index,'unstable operation index');
    if (!plan.batch_across_sheets) check(op.workpiece.sheet_index>=lastSheet,'non-batched sheet order');
    lastSheet=op.workpiece.sheet_index;
    const id=key(op.workpiece), b=op.workpiece.bounds, a=op.action;
    check(live.has(id),'unavailable workpiece');
    equal(op.depends_on,ancestry.get(id).created_by,'incorrect creating dependency');
    if (a.kind!=='handle') equal(loaded,id,'workpiece not loaded');
    if (a.kind==='handle') {check(loaded===null,'uncleared workpiece');loaded=id;counts.handling++;}
    else if (a.kind==='rotate') {
      check(Number.isInteger(a.quarter_turns)&&a.quarter_turns>=0&&a.quarter_turns<4,'invalid orientation');
      check(live.get(id)!==a.quarter_turns,'redundant rotation');live.set(id,a.quarter_turns);counts.rotations++;
    } else if (a.kind==='setup') {
      check(!setup||setup.kind!==a.setup.kind||setup.distance!==a.setup.distance,'redundant setup');
      equal(a.initial,!seen.has(a.setup.kind),'incorrect initial setup');seen.add(a.setup.kind);
      counts[a.initial?'initial_setups':'setup_changes']++; setup=a.setup;
    } else if (a.kind==='cut') {
      check(['vertical','horizontal'].includes(a.axis),'invalid axis');
      const vertical=a.axis==='vertical', k=a.kerf_region;
      const stock=job.problem.sheets[job.stock_indices[op.workpiece.sheet_index]];
      equal(a.kerf,stock.kerf??0,'wrong blade kerf');
      const accessory=plan.machine==='panel_saw'?'panel_stop':vertical?'rip_fence':'crosscut_stop';
      equal(setup,{kind:accessory,distance:a.setting},'wrong physical setup');
      check((vertical?['left','right']:['top','bottom']).includes(a.reference_edge),'wrong reference axis');
      const [lo,hi,start,end]=vertical?[b.x,b.x+b.width,k.x,k.x+k.width]:[b.y,b.y+b.height,k.y,k.y+k.height];
      check(start>=lo&&end<=hi&&start<=end&&start<hi&&end>lo,'invalid blade interval');
      check(end-start<=a.kerf,'blade band too wide');
      if (end-start<a.kerf) check(stock.edge_kerf_relief&&(start===lo||end===hi),'unpermitted partial blade');
      check(a.kerf===0||end>start,'no material consumed by positive blade');
      equal(vertical?[k.y,k.height]:[k.x,k.width],vertical?[b.y,b.height]:[b.x,b.width],'not a full-span pass');
      const low=['left','top'].includes(a.reference_edge);
      equal(a.setting,low?start-lo:hi-end,'wrong workpiece-relative setting');check(a.setting>0,'empty reference side');
      const orientation=live.get(id);
      let normal={left:[-1,0],top:[0,-1],right:[1,0],bottom:[0,1]}[a.reference_edge];
      for (let n=0;n<orientation;n++) normal=[-normal[1],normal[0]];
      equal(normal,[-1,0],'reference does not face guide');
      equal(a.segment,vertical?{from_x:k.x,from_y:b.y,to_x:k.x,to_y:b.y+b.height}
        :{from_x:b.x,from_y:k.y,to_x:b.x+b.width,to_y:k.y},'incorrect diagram segment');
      const expected=[];
      for (const [from,to] of [[lo,start],[end,hi]]) if (to>from) expected.push({sheet_index:op.workpiece.sheet_index,
        bounds:vertical?{...b,x:from,width:to-from}:{...b,y:from,height:to-from}});
      equal(a.children.map(key),expected.map(key),'children are not the exact partition');
      for (const p of job.layouts[op.workpiece.sheet_index].placements) {
        if (p.x>=b.x&&p.y>=b.y&&p.x+p.width<=b.x+b.width&&p.y+p.height<=b.y+b.height)
          check(vertical?p.x+p.width<=start||p.x>=end:p.y+p.height<=start||p.y>=end,'blade intrudes into part');
      }
      live.delete(id);loaded=null;counts.cuts++;
      for (const child of a.children) {check(!ancestry.has(key(child)),'duplicate child identity');
        ancestry.set(key(child),{parent:op.workpiece,created_by:index});live.set(key(child),orientation);}
    } else throw new Error('unknown operation kind');
  }
  check(loaded===null,'dangling load');equal(plan.counts,counts,'incorrect operation counts');
  equal(plan.effort_score,counts.cuts*weights.cut+counts.initial_setups*weights.initial_setup
    +counts.setup_changes*weights.setup_change+counts.rotations*weights.rotation+counts.handling*weights.handling,'wrong effort score');
  equal(plan.workpieces.length,ancestry.size,'missing or extra workpiece records');
  const recorded=new Set();
  for (const piece of plan.workpieces) {
    const id=key(piece.id), b=piece.id.bounds;
    check(!recorded.has(id),'duplicate workpiece record');recorded.add(id);
    check(ancestry.has(id),'unknown workpiece record');
    const expected=ancestry.get(id);
    equal(piece.parent===null?null:key(piece.parent),expected.parent===null?null:key(expected.parent),'wrong parent');
    equal(piece.created_by,expected.created_by,'wrong creation operation');
    const placements=job.layouts[piece.id.sheet_index].placements;
    const part=placements.findIndex(p=>JSON.stringify(box(rect(p)))===JSON.stringify(box(b)));
    equal(piece.finished_part,part<0?null:{sheet_index:piece.id.sheet_index,placement_index:part},'wrong finished part identity');
    const waste=live.has(id)&&!placements.some(p=>p.x<b.x+b.width&&b.x<p.x+p.width&&p.y<b.y+b.height&&b.y<p.y+p.height);
    equal(piece.usable_offcut,waste&&b.width>=(options.min_usable_side??0)&&b.height>=(options.min_usable_side??0),'wrong offcut classification');
  }
  job.layouts.forEach((l,sheet_index)=>l.placements.forEach(p=>check(live.has(key({sheet_index,bounds:rect(p)})),'missing finished part')));
  equal(plan.sheet_views,job.stock_indices.map((stock_index,sheet_index_in_solution)=>({sheet_index_in_solution,stock_index,
    operation_ids:plan.operations.filter(op=>op.workpiece.sheet_index===sheet_index_in_solution).map(op=>op.id)})),'incomplete sheet views');
  check(plan.search.explored_states<=plan.search.limits.max_states,'optimization exceeded limit');
  check(plan.search.baseline_states<=plan.search.limits.baseline_max_states,'construction exceeded limit');
}
if (typeof module!=='undefined') module.exports={replaySawJob};
