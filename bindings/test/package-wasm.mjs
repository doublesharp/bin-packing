import {mkdtemp,readFile,rm,writeFile,mkdir} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const work=await mkdtemp(join(tmpdir(),'bin-packing-wasm-package-'));
try {
  const cwd=fileURLToPath(new URL('../wasm',import.meta.url));
  const npm=process.platform==='win32'?process.execPath:'npm';
  const npmPrefix=process.platform==='win32'?[process.env.npm_execpath?.endsWith('npm-cli.js')?process.env.npm_execpath:join(dirname(process.execPath),'node_modules/npm/bin/npm-cli.js')]:[];
  const packed=Object.values(JSON.parse(execFileSync(npm,[...npmPrefix,'--cache',join(work,'npm-cache'),'pack','--ignore-scripts','--json','--pack-destination',work],{cwd,encoding:'utf8'})))[0];
  const unpack=join(work,'unpack');await mkdir(unpack);execFileSync('tar',['-xzf',join(work,packed.filename),'-C',unpack]);
  const root=join(unpack,'package'),metadata=JSON.parse(await readFile(join(root,'package.json'),'utf8'));
  for(const entry of Object.values(metadata.exports))if(entry.types)await readFile(join(root,entry.types));
  const probe=join(root,'probe.mjs');
  await writeFile(probe,`
import {readFile} from 'node:fs/promises';
import assert from 'node:assert/strict';
import {packageContract} from ${JSON.stringify(new URL('./package-contract.cjs',import.meta.url).href)};
const metadata=JSON.parse(await readFile(new URL('./package.json',import.meta.url),'utf8'));
let checked=0;
for(const [key,entry]of Object.entries(metadata.exports)) {
 if(!entry.types)continue;
 const api=await import(metadata.name+(key==='.'?'':key.slice(1)));
 if(key.endsWith('/web'))await api.default({module_or_path:await readFile(new URL(entry.default.replace(/\\.js$/, '_bg.wasm'),import.meta.url))});
 assert.equal(api.version(),metadata.version,'WASM binary version must match its package');
 const variant=['one-d','two-d','three-d','irregular'].find(d=>key.includes(d));
 packageContract(api,variant?[variant==='irregular'?'irregular':['one-d','two-d','three-d'].indexOf(variant)+1]:[1,2,3,'irregular']);checked++;
}
console.log(JSON.stringify({packed_entrypoints:checked,result:'passed'}));
`);
  execFileSync(process.execPath,['--experimental-wasm-modules',probe],{stdio:'inherit'});
} finally {await rm(work,{recursive:true,force:true});}
