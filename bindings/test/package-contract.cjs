// Browser-compatible contract checks. Deliberately independent of Rust helpers.
function check(condition, message) { if (!condition) throw new Error(message); }
function verifySolution(dimension, problem, solution) {
  const stocks=problem.stock??problem.sheets??problem.bins;
  const remaining=new Map(problem.demands.map(d=>[d.name,d.quantity]));
  check(remaining.size===problem.demands.length,'fixture requires distinct demand names');
  const used=new Map();
  let totalWaste=0, totalCost=0;
  for (const layout of solution.layouts) {
    const name=layout.stock_name??layout.sheet_name??layout.bin_name;
    const stock=stocks.find(s=>s.name===name);
    check(stock,'unknown stock');
    check(layout.cost===(stock.cost??1),'wrong stock cost');
    used.set(name,(used.get(name)??0)+1);
    totalCost+=layout.cost;
    const parts=layout.cuts??layout.placements;
    for (const p of parts) {
      const d=problem.demands.find(d=>d.name===p.name);
      check(d&&remaining.get(p.name)>0,'unexpected or duplicated part');
      remaining.set(p.name,remaining.get(p.name)-1);
      if(dimension===1) check(p.length===d.length,'cut length changed');
      else {
        let extents;
        if(dimension===2) {
          check(!p.rotated||d.can_rotate!==false,'forbidden rotation');
          extents=p.rotated?[d.height,d.width]:[d.width,d.height];
        } else {
          const original={x:d.width,y:d.height,z:d.depth};
          check(['xyz','xzy','yxz','yzx','zxy','zyx'].includes(p.rotation),'invalid rotation');
          const mask=d.allowed_rotations??63;
          check((mask&(1<<['xyz','xzy','yxz','yzx','zxy','zyx'].indexOf(p.rotation)))!==0,'forbidden 3D rotation');
          extents=[...p.rotation].map(axis=>original[axis]);
        }
        for(const [i,[axis,size]]of [['x','width'],['y','height'],['z','depth']].slice(0,dimension).entries()) {
          check(Number.isInteger(p[axis])&&p[axis]>=0&&p[size]===extents[i],'invalid placement dimensions');
          check(p[axis]+p[size]<=stock[size]+(dimension===2&&stock.edge_kerf_relief?(stock.kerf??0):0),'placement outside stock');
        }
      }
    }
    if(dimension===1) {
      const lengths=parts.reduce((s,p)=>s+p.length,0);
      const consumed=lengths+Math.max(0,parts.length-1)*(stock.kerf??0);
      check(consumed<=stock.length-(stock.trim??0),'stock overconsumed');
      check(layout.used_length===consumed,'wrong used length');
      check(layout.remaining_length===stock.length-(stock.trim??0)-consumed&&layout.waste===layout.remaining_length,'wrong remaining length');
    } else {
      for(let i=0;i<parts.length;i++)for(let j=i+1;j<parts.length;j++) {
        const a=parts[i],b=parts[j];
        check([['x','width'],['y','height'],['z','depth']].slice(0,dimension)
          .some(([axis,size])=>a[axis]+a[size]<=b[axis]||b[axis]+b[size]<=a[axis]),'overlapping parts');
        if(dimension===2)for(const [axis,size,other,otherSize]of [['x','width','y','height'],['y','height','x','width']]) {
          if(Math.max(a[other],b[other])<Math.min(a[other]+a[otherSize],b[other]+b[otherSize]))
            check(Math.max(b[axis]-a[axis]-a[size],a[axis]-b[axis]-b[size])>=(stock.kerf??0),'missing kerf between adjacent edges');
        }
      }
    }
    if(dimension>1) {
      const axes=dimension===2?['width','height']:['width','height','depth'];
      check(axes.every(size=>layout[size]===stock[size]),'changed container dimensions');
      const usedArea=parts.reduce((sum,p)=>sum+axes.reduce((v,size,i)=>v*Math.max(0,Math.min(p[size],stock[size]-p[['x','y','z'][i]])),1),0);
      const capacity=axes.reduce((v,size)=>v*stock[size],1);
      check((layout.used_area??layout.used_volume)===usedArea,'wrong used area or volume');
      check((layout.waste_area??layout.waste_volume)===capacity-usedArea,'wrong waste area or volume');
    }
    totalWaste+=layout.waste??layout.waste_area??layout.waste_volume;
  }
  for(const d of solution.unplaced) {
    const source=problem.demands.find(p=>p.name===d.name);
    const quantity=dimension===1?1:d.quantity;
    check(source&&remaining.get(d.name)>=quantity,'unplaced identity mismatch');
    for(const key of ['length','width','height','depth'])if(source[key]!==undefined)check(source[key]===d[key],'unplaced dimensions changed');
    if(dimension===2)check(d.can_rotate===(source.can_rotate??true),'unplaced rotation permission changed');
    remaining.set(d.name,remaining.get(d.name)-quantity);
  }
  check([...remaining.values()].every(n=>n===0),'lost demand');
  for(const stock of stocks)check((used.get(stock.name)??0)<=(stock.available??stock.quantity??Infinity),'inventory exceeded');
  check((solution.stock_count??solution.sheet_count??solution.bin_count)===solution.layouts.length,'wrong container count');
  check((solution.total_waste??solution.total_waste_area??solution.total_waste_volume)===totalWaste,'wrong waste total');
  check(Math.abs(solution.total_cost-totalCost)<=1e-8*Math.max(1,totalCost),'wrong cost total');
}
function mustReject(action, message, pattern) {
  let error;
  try { action(); } catch (caught) { error=caught; }
  check(error!==undefined&&(!pattern||pattern.test(String(error))),message);
}
function verifyStaged(problem, packed) {
  const quantities=problem.packing.problem.demands.map(()=>0),order=[];
  let totalKerf=0;
  check(packed.trees.length===packed.solution.layouts.length,'staged tree/layout count');
  for(const [layoutIndex,tree]of packed.trees.entries()) {
    const stock=problem.packing.problem.sheets[packed.stock_indices[layoutIndex]],trim=problem.packing.constraints?.[packed.stock_indices[layoutIndex]]??{};
    const root={x:trim.trim_left??0,y:trim.trim_top??0,width:stock.width-(trim.trim_left??0)-(trim.trim_right??0),height:stock.height-(trim.trim_top??0)-(trim.trim_bottom??0)};
    const seen=new Set(),parts=[];let kerfArea=0;
    function visit(index,expected,previousAxis,previousStage) {
      check(!seen.has(index),'cycle/shared physical child');seen.add(index);
      const node=tree.nodes[index];check(node,'unknown physical child');
      for(const key of ['x','y','width','height'])check(node.region[key]===expected[key]&&Number.isInteger(expected[key]),'physical partition region');
      const a=node.action,r=node.region;
      if(a.kind==='cut') {
        const stage=previousAxis?previousStage+(previousAxis!==a.axis):a.axis===tree.first_axis?1:2;
        if(problem.rules?.cut_type==='exact'&&problem.rules.max_stages!=null)check(stage<=problem.rules.max_stages,'physical stage bound');
        const vertical=a.axis==='vertical',length=vertical?r.width:r.height,k=stock.kerf??0;
        check(a.offset>=0&&a.offset+k<=length&&(k!==0||a.offset>0&&a.offset<length),'illegal physical cut');
        const expectedChildren=[];
        if(a.offset>0)expectedChildren.push(vertical?{...r,width:a.offset}:{...r,height:a.offset});
        if(a.offset+k<length)expectedChildren.push(vertical?{...r,x:r.x+a.offset+k,width:length-a.offset-k}:{...r,y:r.y+a.offset+k,height:length-a.offset-k});
        check(expectedChildren.length===a.children.length,'physical cut child count');
        kerfArea+=k*(vertical?r.height:r.width);
        check(expectedChildren.reduce((sum,c)=>sum+c.width*c.height,0)+k*(vertical?r.height:r.width)===r.width*r.height,'physical material conservation');
        a.children.forEach((child,i)=>visit(child,expectedChildren[i],a.axis,stage));
      } else if(a.kind==='part') {
        const demand=problem.packing.problem.demands[a.demand_index];check(demand,'unknown physical part source');
        check(r.width===(a.rotated?demand.height:demand.width)&&r.height===(a.rotated?demand.width:demand.height),'part/physical leaf mismatch');
        quantities[a.demand_index]++;parts.push(a.demand_index);order.push({layout_index:layoutIndex,node_index:index});
      } else check(a.kind==='waste','unknown physical leaf');
    }
    visit(0,root,null,0);
    check(seen.size===tree.nodes.length,'unreachable physical operation');
    check(JSON.stringify(parts)===JSON.stringify(packed.demand_indices[layoutIndex]),'physical source provenance');
    check(kerfArea===packed.solution.layouts[layoutIndex].kerf_area,'physical blade loss');totalKerf+=kerfArea;
  }
  check(JSON.stringify(order)===JSON.stringify(packed.production_order),'physical production sequence');
  check(quantities.every((q,i)=>q+packed.unplaced_quantities[i]===problem.packing.problem.demands[i].quantity),'physical demand conservation');
  check(totalKerf===packed.solution.total_kerf_area,'physical total blade loss');
}
function loadingFixture() {
  return {packing:{problem:{bins:[{name:'carton',width:2,height:9,depth:2,quantity:1}],demands:[{name:'nested',width:2,height:6,depth:2,quantity:2,allowed_rotations:1}]},item_weights:[3],bin_max_weights:[6]},
    item_rules:[{stackability_id:7,nesting_height:3,maximum_stackability:2,maximum_weight_above:3}],
    bin_rules:[{maximum_stack_density:{numerator:3,denominator:2},axle_limits:{axis:'z',front_position:0,rear_position:2,front_maximum:4,rear_maximum:5,front_tare:1,rear_tare:2}}]};
}
function extendedContract(api,dimensions,results) {
  const effort={max_evaluations:32,packing_max_states:64,beam_width:4};
  for(const dimension of dimensions.filter(d=>typeof d==='number')) {
    const demands=dimension===1?[{name:'duplicate',length:6,quantity:1},{name:'duplicate',length:5,quantity:2}]:
      [{name:'duplicate',width:6,height:1,...(dimension===3?{depth:1,allowed_rotations:1}:{can_rotate:false}),quantity:1},
       {name:'duplicate',width:5,height:1,...(dimension===3?{depth:1,allowed_rotations:1}:{can_rotate:false}),quantity:2}];
    const source=dimension===1?{stock:[{name:'stock',length:10,available:1}],demands}:
      dimension===2?{sheets:[{name:'stock',width:10,height:1,quantity:1}],demands}:
        {bins:[{name:'stock',width:10,height:1,depth:1,quantity:1}],demands};
    const packing=dimension===1?source:{problem:source};
    const problem={packing,profits:[100,1]};
    const count=api[`solve${dimension}dOptimized`](problem,{...effort,objective:'bin_count'});
    const profit=api[`solve${dimension}dOptimized`](problem,{...effort,objective:'knapsack'});
    check(count.report.objective==='bin_count'&&JSON.stringify(count.report.selected_quantities)==='[0,2]',`${dimension}D count construction must maximize fulfilled copies under finite inventory`);
    const reversed=api[`solve${dimension}dOptimized`]({...problem,profits:[1,100]},{...effort,objective:'knapsack'});
    check(JSON.stringify(reversed.report.selected_quantities)==='[0,2]'&&reversed.report.total_profit===200,'changing source values must change the selected profit subset');
    check(JSON.stringify(profit.report.selected_quantities)==='[1,0]'&&profit.report.total_profit===100,'knapsack must select the valuable source');
    check(profit.stock_indices.length===1&&profit.stock_indices[0]===0&&JSON.stringify(profit.demand_indices)==='[[0]]','optimized duplicate-label source identity');
    check(profit.report.minimums_satisfied&&!profit.report.complete&&profit.report.evaluations<=32,'objective evidence');
    results[`optimized${dimension}d`]=profit;
    mustReject(()=>api[`solve${dimension}dOptimized`](problem,{max_evaluations:0}),'malformed optimization effort must fail');
    mustReject(()=>api[`solve${dimension}dOptimized`](problem,{objecitve:'knapsack'}),'unknown objective options must fail',/unknown field/);
    mustReject(()=>api[`solve${dimension}dOptimized`]({...problem,profits:[Infinity,1]},effort),'non-finite source profit must fail',/finite|safe integer/);
    const nonfinite=dimension===1?{stock:[{name:'stock',length:10,cost:NaN}],demands:[{name:'p',length:1,quantity:1}]}:
      dimension===2?{problem:{sheets:[{name:'stock',width:10,height:1,cost:NaN}],demands:[{name:'p',width:1,height:1,quantity:1}]}}:
        {problem:{bins:[{name:'stock',width:10,height:1,depth:1,cost:NaN}],demands:[{name:'p',width:1,height:1,depth:1,quantity:1}]}};
    mustReject(()=>api[`solve${dimension}dOptimized`]({packing:nonfinite},effort),'non-finite nested material cost must fail',/finite|safe integer/);
    if(dimension===2) {
      const staged={packing, rules:{max_stages:1,first_axis:'vertical',cut_type:'exact'}};
      const selected=api.solve2dStagedOptimized({packing:staged,profits:[100,1]},{...effort,objective:'knapsack'});
      check(JSON.stringify(selected.report.selected_quantities)==='[1,0]','staged profit objective');
      verifyStaged(selected.effective_problem,selected.solution);results.stagedOptimized2d=selected;
    }
    if(dimension===3) {
      const selected=api.solve3dLoadingOptimized({packing:{packing},profits:[100,1]},{...effort,objective:'knapsack'});
      check(JSON.stringify(selected.report.selected_quantities)==='[1,0]','loading profit objective');
      check(selected.solution.layouts[0].stacks[0].items[0].demand_index===0,'loading optimized source identity');
      results.loadingOptimized3d=selected;
    }
  }
  if(dimensions.includes(2)) {
    const dense={sheets:[{name:'stock',width:24,height:18,cost:1}],demands:
      [[12,6,2,true],[9,6,2,true],[9,8,3,false],[8,10,1,false],[13,2,2,true],[9,4,2,true],[8,9,2,false],[12,2,1,false]]
        .map(([width,height,quantity,can_rotate],index)=>({name:`dense_${index}`,width,height,quantity,can_rotate}))};
    const recovered=api.solve2dConstrained({problem:dense},{max_states:4000});
    verifySolution(2,dense,recovered.solution);
    check(recovered.solution.sheet_count===2&&recovered.solution.unplaced.length===0,'dense mixed geometry must recover two sheets within the declared budget');
    check(recovered.explored_states<=4000,'dense recovery must respect its successor budget');
    const staged={packing:{problem:{sheets:[{name:'stock',width:10,height:10}],demands:[{name:'same',width:4,height:3,quantity:1,can_rotate:false},{name:'same',width:4,height:7,quantity:1,can_rotate:false},{name:'same',width:6,height:5,quantity:2,can_rotate:false}]}},rules:{max_stages:2,first_axis:'vertical',cut_type:'exact'}};
    const tree=api.solve2dStaged(staged,{max_states:128});
    check(tree.solution.sheet_count===1&&tree.solution.unplaced.length===0,'staged native construction');
    verifyStaged(staged,tree);results.staged2d=tree;
    mustReject(()=>api.solve2dStaged({...staged,rules:{max_stages:0}}),'invalid stage limit must fail');
    mustReject(()=>api.solve2dStaged({...staged,rules:{max_stage:1}}),'misspelled stage constraints must fail',/unknown field/);
    mustReject(()=>api.solve2dConstrained({...staged.packing,constraints:[{trim_lefft:1}]}),'misspelled physical trims must fail',/unknown field/);
    mustReject(()=>api.solve2dStagedOptimized({packing:staged},{max_evaluations:0}),'invalid staged objective control must fail');
  }
  if(dimensions.includes(3)) {
    const loading=loadingFixture(),stacked=api.solve3dLoading(loading,{max_states:64});
    check(stacked.bin_count===1&&stacked.unplaced.length===0,'nested loading fulfillment');
    const layout=stacked.layouts[0],stack=layout.stacks[0];
    check(layout.bin_index===0&&layout.payload===6&&stack.weight===6&&stack.height===9,'nested stack geometry and payload');
    check(stack.items.length===2&&stack.items[0].y===0&&stack.items[1].y===3&&stack.items.every(i=>i.demand_index===0&&i.rotation==='xyz'),'nested source transforms');
    check(layout.axle_loads.front===4&&layout.axle_loads.rear===5,'exact axle reaction readback');
    results.loading3d=stacked;
    for(const value of [NaN,Infinity,-Infinity,undefined,Number.MAX_SAFE_INTEGER+1]) {
      const invalid=loadingFixture();invalid.item_rules[0].maximum_weight_above=value;
      mustReject(()=>api.solve3dLoading(invalid),'invalid nullable stack limit must fail',/safe integer/);
      const invalidDensity=loadingFixture();invalidDensity.bin_rules[0].maximum_stack_density.numerator=value;
      mustReject(()=>api.solve3dLoadingOptimized({packing:invalidDensity},effort),'invalid nested density must fail',/safe integer/);
      const invalidAxle=loadingFixture();invalidAxle.bin_rules[0].axle_limits.front_maximum=value;
      mustReject(()=>api.solve3dLoading(invalidAxle),'invalid axle load must fail',/safe integer/);
      const invalidWeight=loadingFixture();invalidWeight.packing.bin_max_weights=[value];
      mustReject(()=>api.solve3dLoadingOptimized({packing:invalidWeight},effort),'invalid nested payload limit must fail',/safe integer/);
    }
    const overflow=loadingFixture();overflow.packing.item_weights=[Number.MAX_SAFE_INTEGER];overflow.packing.bin_max_weights=[];
    overflow.item_rules[0].maximum_weight_above=null;overflow.bin_rules=[];
    mustReject(()=>api.solve3dLoading(overflow),'unsafe aggregate output payload must fail',/safe integer/);
    mustReject(()=>api.solve3dLoadingOptimized({packing:overflow},effort),'unsafe optimized output payload must fail',/safe integer/);
    mustReject(()=>api.solve3dLoading(loading,{beam_width:0}),'invalid loading controls must fail');
    const customJson={toJSON(){const value=loadingFixture();value.packing.bin_max_weights=[Infinity];return value;}};
    mustReject(()=>api.solve3dLoading(customJson),'custom JSON must not convert an infinite limit to unlimited',/finite|safe integer/);
    const unsafeOptimized={packing:{problem:{bins:[{name:'bin',width:2,height:1,depth:1,quantity:1}],demands:[{name:'p',width:1,height:1,depth:1,quantity:2,allowed_rotations:1}]},item_weights:[Number.MAX_SAFE_INTEGER]}};
    mustReject(()=>api.solve3dOptimized(unsafeOptimized,effort),'unsafe optimized spatial payload must fail',/safe integer/);
  }
  if(dimensions.includes('irregular')) {
    const shape=vertices=>({components:[{exterior:{kind:'polygon',vertices:vertices.map(([x,y])=>({x,y}))}}]});
    const problem={bins:[{name:'stock',shape:shape([[0,0],[2,0],[2,3],[0,3]]),quantity:1}],demands:[{name:'L',shape:shape([[0,0],[2,0],[2,1],[1,1],[1,2],[0,2]]),quantity:2,allowed_rotations:[{start:0,end:180}]}]};
    const nested=api.solveIrregular(problem);
    check(nested.feasible&&nested.layouts.length===1&&nested.layouts[0].placements.length===2,'concave L material must interlock in one bin');
    check(nested.layouts[0].used_area===6&&nested.unplaced_quantities[0]===0,'irregular material/source conservation');
    for(const placement of nested.layouts[0].placements) {
      check(placement.demand_index===0&&!placement.mirrored,'irregular source transform identity');
      const angle=placement.rotation_degrees*Math.PI/180;
      for(const point of problem.demands[0].shape.components[0].exterior.vertices) {
        const x=placement.x+point.x*Math.cos(angle)-point.y*Math.sin(angle),y=placement.y+point.x*Math.sin(angle)+point.y*Math.cos(angle);
        check(x>=-1e-8&&x<=2+1e-8&&y>=-1e-8&&y<=3+1e-8,'irregular original transform outside source bin');
      }
    }
    results.irregular=nested;
    if(api.solveIrregularJson)check(JSON.stringify(JSON.parse(api.solveIrregularJson(JSON.stringify(problem))))===JSON.stringify(nested),'irregular JSON/object disagreement');
    const open=api.solveIrregular(problem,{objective:'open_dimension_xy',max_states:0});
    check(open.options.objective==='open_dimension_xy','irregular open XY wire spelling');
    mustReject(()=>api.solveIrregular(problem,{curve_tolerance:0}),'invalid irregular geometry controls must fail');
    mustReject(()=>api.solveIrregular(problem,{item_spacing:Infinity}),'non-finite irregular controls must fail',/finite/);
    const bad={...problem,bins:[{...problem.bins[0],spacing:NaN}]};
    mustReject(()=>api.solveIrregular(bad),'non-finite irregular material spacing must fail',/finite/);
  }
}
function packageContract(api, dimensions=[1,2,3,'irregular']) {
  const results={};
  check(typeof api.version()==='string','version contract');
  for(const [dimension,names]of [[1,['solve1dOptimized']],[2,['solve2dOptimized','solve2dStaged','solve2dStagedOptimized']],[3,['solve3dOptimized','solve3dLoading','solve3dLoadingOptimized']],['irregular',['solveIrregular']]]) {
    if(dimensions.includes(dimension))for(const name of names)check(typeof api[name]==='function',`missing public ${name} entry point`);
  }
  for(const dimension of dimensions) {
    if(dimension==='irregular')continue;
    const problem=dimension===1?{stock:[{name:'stock',length:47,kerf:2,trim:1}],demands:[{name:'a',length:11,quantity:2},{name:'b',length:7,quantity:1}]}:
      dimension===2?{sheets:[{name:'stock',width:47,height:31,kerf:2}],demands:[{name:'a',width:11,height:7,quantity:2,can_rotate:true},{name:'b',width:9,height:13,quantity:1,can_rotate:false}]}:
        {bins:[{name:'stock',width:23,height:19,depth:17}],demands:[{name:'a',width:3,height:5,depth:7,quantity:2,allowed_rotations:63}]};
    const options={algorithm:dimension===1?'best_fit_decreasing':dimension===2?'guillotine':'extreme_points',seed:42};
    const solution=api[`solve${dimension}d`](problem,options);
    verifySolution(dimension,problem,solution);
    if(dimension===1)check(api.plan1dCuts(problem,solution).bar_plans.length===solution.layouts.length,'1D cut plan');
    if(dimension===2)check(api.plan2dCuts(solution).sheet_plans.length===solution.layouts.length,'2D cut plan');
    const json=api[`solve${dimension}dJson`];
    if(json)check(JSON.stringify(JSON.parse(json(JSON.stringify(problem),JSON.stringify(options))))===JSON.stringify(solution),'JSON/object disagreement');
    results[dimension]=solution;
  }
  if(dimensions.includes(1)) {
    const priced={stock:[{name:'large',length:10,cost:10},{name:'small',length:5,cost:1}],demands:[{name:'cut',length:5,quantity:2}]};
    const cheap=api.solve1dMinCost(priced);
    verifySolution(1,priced,cheap);
    check(cheap.exact&&cheap.stock_count===2&&cheap.total_cost===2,'cost-first linear stock');
    results.minCost1d=cheap;
  }
  if(dimensions.includes(2)) {
    const problem={sheets:[{name:'large',width:1000000,height:1000000}],demands:[{name:'p',width:1,height:1,quantity:1,can_rotate:false}]};
    const solution=api.solve2d(problem,{algorithm:'guillotine'});
    check(solution.total_sum_sq_usable_drop_areas>1e21,'large metric fixture');
    check(api.plan2dCuts(solution).sheet_plans.length===1,'large solve-to-cut roundtrip');
    const bounded={sheets:[{name:'sheet',width:10,height:10}],demands:[{name:'p',width:5,height:5,quantity:4,can_rotate:false}]};
    const adaptive=api.solve2dAdaptive(bounded);
    verifySolution(2,bounded,adaptive.solution);
    check(api.lowerBound2d(bounded)===1&&adaptive.lower_bound===1,'count lower bound');
    check(adaptive.count_optimal&&adaptive.candidates_run===1&&adaptive.stopped_on_bound,'adaptive count certificate');
    results.adaptive2d=adaptive;
    const constrained={problem:{sheets:[{name:'sheet',width:10,height:10}],demands:[{name:'p',width:2,height:2,quantity:4,can_rotate:false}]},constraints:[{trim_left:1,trim_right:1,trim_top:1,trim_bottom:1,defects:[{x:4,y:4,width:2,height:2}]}]};
    const planned=api.solve2dConstrained(constrained,{max_states:0});
    verifySolution(2,constrained.problem,planned.solution);
    check(planned.stock_indices.length===planned.solution.layouts.length,'2D stock provenance');
    for(const [i,layout] of planned.solution.layouts.entries()) {
      check(planned.stock_indices[i]===0,'2D stock source');
      check(planned.demand_indices[i].length===layout.placements.length,'2D demand provenance');
      for(const [j,p] of layout.placements.entries()) {
        check(planned.demand_indices[i][j]===0,'2D demand source');
        check(p.x>=1&&p.y>=1&&p.x+p.width<=9&&p.y+p.height<=9,'trim violation');
        check(p.x+p.width<=4||p.x>=6||p.y+p.height<=4||p.y>=6,'defect intersection');
      }
    }
    check(planned.solution.unplaced.length===0,'constrained sheet failed to place demands');
    results.constrained2d=planned;
    const priced={problem:{sheets:[{name:'large',width:8,height:4,cost:10},{name:'small',width:4,height:4,cost:1}],demands:[{name:'p',width:4,height:4,quantity:2,can_rotate:false}]}};
    const cheap=api.solve2dConstrained(priced,{objective:'total_cost'});
    verifySolution(2,priced.problem,cheap.solution);
    check(cheap.solution.total_cost===2,'cost-first construction');
  }
  if(dimensions.includes(3)) {
    const constrained={problem:{bins:[{name:'carton',width:10,height:10,depth:10}],demands:[{name:'p',width:5,height:5,depth:5,quantity:2,allowed_rotations:1}]},item_weights:[6],bin_max_weights:[10]};
    const packed=api.solve3dConstrained(constrained,{max_states:0});
    verifySolution(3,constrained.problem,packed.solution);
    check(packed.solution.bin_count===2&&packed.solution.unplaced.length===0,'payload capacity');
    check(packed.bin_indices.length===2&&packed.bin_weights.length===2,'3D provenance shape');
    for(const [i,layout] of packed.solution.layouts.entries()) {
      check(packed.bin_indices[i]===0&&packed.bin_weights[i]===6,'3D payload readback');
      check(packed.demand_indices[i].length===layout.placements.length&&packed.demand_indices[i].every(index=>index===0),'3D demand provenance');
    }
    results.constrained3d=packed;
    for(const input of [
      {...constrained,item_weights:[Number.MAX_SAFE_INTEGER+1]},
      {...constrained,item_weights:[Number.MAX_SAFE_INTEGER],bin_max_weights:[]},
      ...[NaN,Infinity,-Infinity,undefined].map(weight=>({...constrained,bin_max_weights:[weight]}))
    ]) {
      let rejected=false;
      try {api.solve3dConstrained(input,{max_states:0});} catch(error) {rejected=/safe integer/.test(String(error));}
      check(rejected,'unsafe JavaScript weight must fail explicitly');
    }
  }
  extendedContract(api,dimensions,results);
  return results;
}
if(typeof module!=='undefined')module.exports={verifySolution,packageContract};
