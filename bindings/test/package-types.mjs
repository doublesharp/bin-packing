// Compile real runtime output against every public package declaration.
import {createRequire} from 'node:module';
import {readFile,writeFile,mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
const require=createRequire(import.meta.url);
const {packageContract}=require('./package-contract.cjs');
const base=fileURLToPath(new URL('..',import.meta.url));
const metadata=JSON.parse(await readFile(join(base,'wasm/package.json'),'utf8'));
function signatureSource(alias, values) {
  const declarations=[
    ['optimized1d','solve1dOptimized','OptimizationProblem1D','OptimizationOptions','OptimizedSolution1D'],
    ['optimized2d','solve2dOptimized','OptimizationProblem2D','OptimizationOptions','OptimizedSolution2D'],
    ['optimized3d','solve3dOptimized','OptimizationProblem3D','OptimizationOptions','OptimizedSolution3D'],
    ['staged2d','solve2dStaged','StagedProblem2D','StagedOptions2D','StagedSolution2D'],
    ['stagedOptimized2d','solve2dStagedOptimized','OptimizationProblemStaged2D','OptimizationOptions','OptimizedSolutionStaged2D'],
    ['loading3d','solve3dLoading','LoadingProblem3D','LoadingOptions3D','LoadingSolution3D'],
    ['loadingOptimized3d','solve3dLoadingOptimized','OptimizationProblemLoading3D','OptimizationOptions','OptimizedSolutionLoading3D'],
    ['irregular','solveIrregular','IrregularProblem','IrregularOptions','IrregularSolution'],
  ];
  return declarations.filter(([key])=>values[key]).map(([,fn,input,options,output])=>`const ${alias}${fn}: (problem: ${alias}.${input}, options?: ${alias}.${options} | null) => ${alias}.${output} = ${alias}.${fn};\n`).join('');
}
const work=await mkdtemp(join(tmpdir(),'bin-packing-types-'));
try {
  let source='';let index=0;
  for(const [subpath,entry]of Object.entries(metadata.exports)) {
    if(!entry.types)continue;
    const prefix=['one-d','two-d','three-d','irregular'].find(d=>subpath.includes(d));
    const dimensions=prefix?[prefix==='irregular'?'irregular':['one-d','two-d','three-d'].indexOf(prefix)+1]:[1,2,3,'irregular'];
    const api=require(join(base,`wasm/dist/${prefix?prefix+'/':''}nodejs/bin_packing_wasm.cjs`));
    const values=packageContract(api,dimensions);
    const alias=`P${index++}`;
    source+=`import * as ${alias} from ${JSON.stringify(resolve(base,'wasm',entry.default))};\n`;
    for(const dimension of dimensions.filter(d=>typeof d==='number'))source+=`const ${alias}D${dimension}: ${alias}.${['','OneD','TwoD','ThreeD'][dimension]}Solution = ${JSON.stringify(values[dimension])};\n`;
    if(dimensions.includes(1))source+=`const ${alias}MinCost1D: ${alias}.OneDSolution = ${JSON.stringify(values.minCost1d)};\n`;
    if(dimensions.includes(2))source+=`function ${alias}SawCounters(report: ${alias}.SawSearchReport2D): [number,number,boolean] { return [report.dominated_states,report.peak_dominance_units,report.dominance_table_saturated]; }\n`;
    if(dimensions.includes(2))source+=`const ${alias}Constrained2D: ${alias}.ConstrainedSolution2D = ${JSON.stringify(values.constrained2d)};\nconst ${alias}Adaptive2D: ${alias}.AdaptiveSolution2D = ${JSON.stringify(values.adaptive2d)};\n`;
    if(dimensions.includes(3))source+=`const ${alias}Constrained3D: ${alias}.ConstrainedSolution3D = ${JSON.stringify(values.constrained3d)};\n`;
    for(const [key,type]of [['optimized1d','OptimizedSolution1D'],['optimized2d','OptimizedSolution2D'],['optimized3d','OptimizedSolution3D'],['staged2d','StagedSolution2D'],['stagedOptimized2d','OptimizedSolutionStaged2D'],['loading3d','LoadingSolution3D'],['loadingOptimized3d','OptimizedSolutionLoading3D'],['irregular','IrregularSolution']]) {
      if(values[key])source+=`const ${alias}${key}: ${alias}.${type} = ${JSON.stringify(values[key])};\n`;
    }
    if(dimensions.includes('irregular'))source+=`const ${alias}XY: ${alias}.IrregularObjective = 'open_dimension_xy';\n`;
    source+=signatureSource(alias,values);
    if(subpath.endsWith('/web')) {
      source+=`const ${alias}Raw = ${alias}.initSync({module:new Uint8Array()});\nconst ${alias}Version: [number,number] = ${alias}Raw.version();\n`;
      for(const d of dimensions.filter(d=>typeof d==='number'))source+=`// @ts-expect-error raw ABI results are not ergonomic solution objects\nconst ${alias}Wrong${d}: ${alias}.${['','OneD','TwoD','ThreeD'][d]}Solution = ${alias}Raw.solve${d}d({},{});\n`;
      if(dimensions.includes('irregular'))source+=`// @ts-expect-error raw irregular ABI is not the ergonomic object API\nconst ${alias}WrongIrregular: ${alias}.IrregularSolution = ${alias}Raw.solveIrregular({},{});\n`;
    }
  }
  source+=`import * as Native from ${JSON.stringify(join(base,'node/wrapper'))};\n`;
  const values=packageContract(require('../node'));
  for(const d of [1,2,3])source+=`const Native${d}: Native.${['','OneD','TwoD','ThreeD'][d]}Solution = ${JSON.stringify(values[d])};\n`;
  source+=`const NativeMinCost1D: Native.OneDSolution = ${JSON.stringify(values.minCost1d)};\n`;
  source+=`function NativeSawCounters(report: Native.SawSearchReport2D): [number,number,boolean] { return [report.dominated_states,report.peak_dominance_units,report.dominance_table_saturated]; }\n`;
  source+=`const NativeConstrained2D: Native.ConstrainedSolution2D = ${JSON.stringify(values.constrained2d)};\nconst NativeAdaptive2D: Native.AdaptiveSolution2D = ${JSON.stringify(values.adaptive2d)};\nconst NativeConstrained3D: Native.ConstrainedSolution3D = ${JSON.stringify(values.constrained3d)};\n`;
  for(const [key,type]of [['optimized1d','OptimizedSolution1D'],['optimized2d','OptimizedSolution2D'],['optimized3d','OptimizedSolution3D'],['staged2d','StagedSolution2D'],['stagedOptimized2d','OptimizedSolutionStaged2D'],['loading3d','LoadingSolution3D'],['loadingOptimized3d','OptimizedSolutionLoading3D'],['irregular','IrregularSolution']])source+=`const Native${key}: Native.${type} = ${JSON.stringify(values[key])};\n`;
  source+=signatureSource('Native',values);
  const file=join(work,'contract.mts');await writeFile(file,source);
  const args=['--strict','--noEmit','--lib','es2022,dom','--moduleResolution','bundler','--module','esnext',file];
  if(process.argv[2])execFileSync(process.execPath,[resolve(process.argv[2]),...args],{stdio:'inherit'});
  else execFileSync('tsc',args,{stdio:'inherit'});
  console.log(`Type-checked ${index+1} package declarations against runtime values, saw counters and raw web ABI.`);
} finally {await rm(work,{recursive:true,force:true});}
