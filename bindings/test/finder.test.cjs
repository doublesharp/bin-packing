const test=require('node:test');
const assert=require('node:assert/strict');
const {generate,runCase}=require('./solver-finder.cjs');
const native=require('../node');
test('finder cases reproduce from seed and exercise each dimension',()=>{
  assert.deepEqual(generate(42,15),generate(42,15));
  assert.notDeepEqual(generate(42,15),generate(43,15));
  assert.deepEqual([...new Set(generate(42,15).map(x=>x.dimension))],[1,2,3]);
});
test('finder rejects lost demand and false rotation metadata',()=>{
  const input={dimension:2,problem:{sheets:[{name:'s',width:2,height:1}],demands:[{name:'p',width:1,height:2,quantity:1,can_rotate:true}]},options:{algorithm:'rotation_search'}};
  runCase(input,[['native',native]]);
  assert.throws(()=>runCase(input,[['broken',{solve2d:(...args)=>{
    const s=native.solve2d(...args);s.layouts[0].placements[0].rotated=false;return s;
  }}]]),/placement dimensions/);
  assert.throws(()=>runCase(input,[['broken',{solve2d:(...args)=>{
    const s=native.solve2d(...args);s.layouts=[];s.sheet_count=0;s.total_waste_area=0;s.total_cost=0;return s;
  }}]]),/lost demand/);
});

test('kerf checker permits diagonal separation and rejects adjacent narrow gaps',()=>{
  const {verifySolution}=require('./package-contract.cjs');
  const problem={sheets:[{name:'s',width:8,height:8,kerf:2}],demands:[{name:'a',width:2,height:2,quantity:1,can_rotate:false},{name:'b',width:2,height:2,quantity:1,can_rotate:false}]};
  const make=(x,y)=>({sheet_count:1,total_waste_area:56,total_cost:1,unplaced:[],layouts:[{sheet_name:'s',width:8,height:8,cost:1,used_area:8,waste_area:56,placements:[{name:'a',x:0,y:0,width:2,height:2,rotated:false},{name:'b',x,y,width:2,height:2,rotated:false}]}]});
  verifySolution(2,problem,make(3,3));
  assert.throws(()=>verifySolution(2,problem,make(3,0)),/missing kerf/);
});
