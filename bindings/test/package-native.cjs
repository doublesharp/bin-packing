// Verify the publishable tarball in isolation, without relying on checkout files.
const {execFileSync}=require('node:child_process');
const {mkdtempSync,rmSync,readFileSync,mkdirSync}=require('node:fs');
const {tmpdir}=require('node:os');
const path=require('node:path');
const {packageContract}=require('./package-contract.cjs');
const work=mkdtempSync(path.join(tmpdir(),'bin-packing-package-'));
try {
  const cwd=path.resolve(__dirname,'../node');
  const npm=process.platform==='win32'?process.execPath:'npm';
  const npmPrefix=process.platform==='win32'?[process.env.npm_execpath?.endsWith('npm-cli.js')?process.env.npm_execpath:path.join(path.dirname(process.execPath),'node_modules/npm/bin/npm-cli.js')]:[];
  const packed=JSON.parse(execFileSync(npm,[...npmPrefix,'--cache',path.join(work,'npm-cache'),'pack','--ignore-scripts','--json','--pack-destination',work],{cwd,encoding:'utf8'}))[0];
  for(const required of ['index.js','wrapper.js','wrapper.d.ts'])if(!packed.files.some(f=>f.path===required))throw new Error(`native package missing ${required}`);
  const unpack=path.join(work,'unpack');mkdirSync(unpack);
  execFileSync('tar',['-xzf',path.join(work,packed.filename),'-C',unpack]);
  const api=require(path.join(unpack,'package'));
  packageContract(api);
  console.log(JSON.stringify({package:JSON.parse(readFileSync(path.join(unpack,'package/package.json'),'utf8')).name,packed_files:packed.files.length,contract:'passed'}));
} finally {rmSync(work,{recursive:true,force:true});}
