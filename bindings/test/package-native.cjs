// Verify the publishable tarball in isolation, without relying on checkout files.
const {execFileSync}=require('node:child_process');
const {mkdtempSync,rmSync,readFileSync,mkdirSync}=require('node:fs');
const {tmpdir}=require('node:os');
const assert=require('node:assert/strict');
const path=require('node:path');
const {packageContract}=require('./package-contract.cjs');
const work=mkdtempSync(path.join(tmpdir(),'bin-packing-package-'));
try {
  const cwd=path.resolve(__dirname,'../node');
  const npm=process.platform==='win32'?process.execPath:'npm';
  const npmPrefix=process.platform==='win32'?[process.env.npm_execpath?.endsWith('npm-cli.js')?process.env.npm_execpath:path.join(path.dirname(process.execPath),'node_modules/npm/bin/npm-cli.js')]:[];
  const packed=Object.values(JSON.parse(execFileSync(npm,[...npmPrefix,'--cache',path.join(work,'npm-cache'),'pack','--ignore-scripts','--json','--pack-destination',work],{cwd,encoding:'utf8'})))[0];
  for(const required of ['index.js','wrapper.js','wrapper.d.ts'])if(!packed.files.some(f=>f.path===required))throw new Error(`native package missing ${required}`);
  const unpack=path.join(work,'unpack');mkdirSync(unpack);
  execFileSync('tar',['-xzf',path.join(work,packed.filename),'-C',unpack]);
  const api=require(path.join(unpack,'package'));
  packageContract(api);
  const manifest=JSON.parse(readFileSync(path.join(unpack,'package/package.json'),'utf8'));
  assert.equal(api.version(),manifest.version,'native binary version must match its package');
  if(process.argv.includes('--release')) {
    const {parseTriple}=require('../node/node_modules/@napi-rs/cli');
    const targets=manifest.napi.targets.map(parseTriple);
    const expected=Object.fromEntries(targets.map(t=>[`${manifest.name}-${t.platformArchABI}`,manifest.version]));
    assert.deepEqual(manifest.optionalDependencies,expected,'wrapper must depend on every exact-version native package');
    for(const target of targets) {
      const dir=path.join(cwd,'npm',target.platformArchABI);
      const native=JSON.parse(readFileSync(path.join(dir,'package.json'),'utf8'));
      assert.equal(native.name,`${manifest.name}-${target.platformArchABI}`);
      assert.equal(native.version,manifest.version);
      const binary=`${manifest.napi.binaryName}.${target.platformArchABI}.node`;
      assert.equal(native.main,binary);
      assert.ok(readFileSync(path.join(dir,binary)).length>0,'native artifact must be present');
      const tar=Object.values(JSON.parse(execFileSync(npm,[...npmPrefix,'--cache',path.join(work,'npm-cache'),'pack','--ignore-scripts','--json','--pack-destination',work],{cwd:dir,encoding:'utf8'})))[0];
      assert.ok(tar.files.some(file=>file.path===binary),'platform tarball must include its binary');
      const destination=path.join(unpack,'package/node_modules',native.name);
      mkdirSync(destination,{recursive:true});
      execFileSync('tar',['-xzf',path.join(work,tar.filename),'-C',destination,'--strip-components=1']);
      rmSync(path.join(unpack,'package',binary));
    }
    execFileSync(process.execPath,['-e',`const api=require(${JSON.stringify(path.join(unpack,'package'))});require(${JSON.stringify(path.join(__dirname,'package-contract.cjs'))}).packageContract(api);require('node:assert/strict').equal(api.version(),${JSON.stringify(manifest.version)});`],{stdio:'inherit'});
    console.log('All native tarballs validated; wrapper loaded through platform dependencies');
  }
  console.log(JSON.stringify({package:JSON.parse(readFileSync(path.join(unpack,'package/package.json'),'utf8')).name,packed_files:packed.files.length,contract:'passed'}));
} finally {rmSync(work,{recursive:true,force:true});}
