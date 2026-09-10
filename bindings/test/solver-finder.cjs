#!/usr/bin/env node
// Seeded differential and invariant checks; save the first counterexample for replay.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {verifySolution}=require('./package-contract.cjs');
const algorithms={1:['first_fit_decreasing','best_fit_decreasing','local_search','auto'],
  2:['max_rects','skyline','guillotine','rotation_search','auto'],
  3:['extreme_points','guillotine_3d','layer_building','wall_building','auto']};
function generate(seed,cases) {
  let state=seed>>>0;
  const next=n=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state%n;};
  return Array.from({length:cases},(_,index)=>{
    const dimension=index%3+1,types=1+next(6);
    const demands=Array.from({length:types},(_,i)=>({name:`p${i}`,quantity:1+next(3),...(dimension===1?{length:1+next(18)}:
      {width:1+next(9),height:1+next(11),...(dimension===3?{depth:1+next(7),allowed_rotations:63}:{can_rotate:!!next(2)})})}));
    const cap=next(4)===0?next(3):null;
    const problem=dimension===1?{stock:[{name:'s',length:47,kerf:next(3),trim:next(3),available:cap}],demands}:
      dimension===2?{sheets:[{name:'s',width:37,height:29,kerf:next(3),quantity:cap}],demands}:
        {bins:[{name:'s',width:19,height:17,depth:13,quantity:cap}],demands};
    return {dimension,problem,options:{algorithm:algorithms[dimension][Math.floor(index/3)%algorithms[dimension].length],
      seed:next(100000),multistart_runs:3,improvement_rounds:3,beam_width:4,auto_rotation_search_max_types:3}};
  });
}
function runCase(input,apis) {
  let first;
  for(const [name,api]of apis) {
    const solution=api[`solve${input.dimension}d`](input.problem,input.options);
    try {verifySolution(input.dimension,input.problem,solution);}catch(error){throw new Error(`${name}: ${error.message}`,{cause:error});}
    assert.deepEqual(solution,api[`solve${input.dimension}d`](input.problem,input.options),`${name}: seeded replay differs`);
    // SmallRng intentionally differs on 32-bit WASM and 64-bit native targets.
    // Compare deterministic constructors across backends; independently validate
    // sampled solvers and require reproducibility within each backend.
    const deterministic=['first_fit_decreasing','best_fit_decreasing','max_rects','skyline','guillotine','extreme_points','guillotine_3d'].includes(input.options.algorithm);
    if(first&&deterministic)assert.deepEqual(solution,first,`${name}: native/WASM result differs`);
    else first=solution;
  }
}
function main() {
  const args=process.argv.slice(2),options={seed:20260909,cases:300,output:'fuzz/artifacts/solver-finder.json'};
  for(let i=0;i<args.length;i+=2) {
    const key=args[i].replace(/^--/,'');
    if(!['seed','cases','output','replay'].includes(key)||args[i+1]===undefined)throw new Error('Usage: solver-finder.cjs [--seed N] [--cases 1..10000] [--output file] [--replay file]');
    options[key]=['seed','cases'].includes(key)?Number(args[i+1]):args[i+1];
  }
  if(!Number.isInteger(options.seed)||options.seed<0||options.seed>0xffffffff||!Number.isInteger(options.cases)||options.cases<1||options.cases>10000)throw new Error('seed must be u32; cases must be 1..10000');
  const native=require('../node');
  const combined=require('../wasm/dist/nodejs/bin_packing_wasm.cjs');
  const specialized=[null,...['one-d','two-d','three-d'].map(d=>require(`../wasm/dist/${d}/nodejs/bin_packing_wasm.cjs`))];
  const inputs=options.replay?[JSON.parse(fs.readFileSync(options.replay,'utf8')).input]:generate(options.seed,options.cases);
  for(const [index,input]of inputs.entries()) {
    try {runCase(input,[['native',native],['combined',combined],['specialized',specialized[input.dimension]]]);}
    catch(error) {
      fs.mkdirSync(path.dirname(options.output),{recursive:true});
      fs.writeFileSync(options.output,JSON.stringify({seed:options.seed,index,input,error:String(error)},null,2)+'\n');
      throw new Error(`Case ${index} failed; replay with --replay ${options.output}: ${error.message}`,{cause:error});
    }
  }
  console.log(JSON.stringify({seed:options.seed,cases:inputs.length,solver_calls:inputs.length*6,result:'passed'}));
}
if(require.main===module){try{main();}catch(error){console.error(error);process.exitCode=1;}}
module.exports={generate,runCase};
