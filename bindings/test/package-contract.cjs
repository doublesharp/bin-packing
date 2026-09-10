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
function packageContract(api, dimensions=[1,2,3]) {
  const results={};
  check(typeof api.version()==='string','version contract');
  for(const dimension of dimensions) {
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
  if(dimensions.includes(2)) {
    const problem={sheets:[{name:'large',width:1000000,height:1000000}],demands:[{name:'p',width:1,height:1,quantity:1,can_rotate:false}]};
    const solution=api.solve2d(problem,{algorithm:'guillotine'});
    check(solution.total_sum_sq_usable_drop_areas>1e21,'large metric fixture');
    check(api.plan2dCuts(solution).sheet_plans.length===1,'large solve-to-cut roundtrip');
  }
  return results;
}
if(typeof module!=='undefined')module.exports={verifySolution,packageContract};
