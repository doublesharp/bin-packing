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
const work=await mkdtemp(join(tmpdir(),'bin-packing-types-'));
try {
  let source='';let index=0;
  for(const [subpath,entry]of Object.entries(metadata.exports)) {
    if(!entry.types)continue;
    const prefix=['one-d','two-d','three-d'].find(d=>subpath.includes(d));
    const dimensions=prefix?[['one-d','two-d','three-d'].indexOf(prefix)+1]:[1,2,3];
    const api=require(join(base,`wasm/dist/${prefix?prefix+'/':''}nodejs/bin_packing_wasm.cjs`));
    const values=packageContract(api,dimensions);
    const alias=`P${index++}`;
    source+=`import * as ${alias} from ${JSON.stringify(resolve(base,'wasm',entry.default))};\n`;
    for(const dimension of dimensions)source+=`const ${alias}D${dimension}: ${alias}.${['','OneD','TwoD','ThreeD'][dimension]}Solution = ${JSON.stringify(values[dimension])};\n`;
    if(dimensions.includes(2))source+=`function ${alias}SawCounters(report: ${alias}.SawSearchReport2D): [number,number,boolean] { return [report.dominated_states,report.peak_dominance_units,report.dominance_table_saturated]; }\n`;
    if(subpath.endsWith('/web')) {
      source+=`const ${alias}Raw = ${alias}.initSync({module:new Uint8Array()});\nconst ${alias}Version: [number,number] = ${alias}Raw.version();\n`;
      for(const d of dimensions)source+=`// @ts-expect-error raw ABI results are not ergonomic solution objects\nconst ${alias}Wrong${d}: ${alias}.${['','OneD','TwoD','ThreeD'][d]}Solution = ${alias}Raw.solve${d}d({},{});\n`;
    }
  }
  source+=`import * as Native from ${JSON.stringify(join(base,'node/wrapper'))};\n`;
  const values=packageContract(require('../node'));
  for(const d of [1,2,3])source+=`const Native${d}: Native.${['','OneD','TwoD','ThreeD'][d]}Solution = ${JSON.stringify(values[d])};\n`;
  source+=`function NativeSawCounters(report: Native.SawSearchReport2D): [number,number,boolean] { return [report.dominated_states,report.peak_dominance_units,report.dominance_table_saturated]; }\n`;
  const file=join(work,'contract.mts');await writeFile(file,source);
  const args=['--strict','--noEmit','--lib','es2022,dom','--moduleResolution','bundler','--module','esnext',file];
  if(process.argv[2])execFileSync(process.execPath,[resolve(process.argv[2]),...args],{stdio:'inherit'});
  else execFileSync('tsc',args,{stdio:'inherit'});
  console.log(`Type-checked ${index+1} package declarations against runtime values, saw counters and raw web ABI.`);
} finally {await rm(work,{recursive:true,force:true});}
