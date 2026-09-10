import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {packageContract} from '../../test/package-contract.cjs';

for(const [variant,dimensions]of [['',[1,2,3]],['one-d',[1]],['two-d',[2]],['three-d',[3]]]) {
  for(const flavor of ['bundler','web','nodejs']) {
    test(`${variant||'combined'}/${flavor} public package contract`,async()=>{
      const subpath=[variant,flavor==='bundler'?'':flavor].filter(Boolean).join('/');
      const api=await import(`@0xdoublesharp/bin-packing-wasm${subpath?'/'+subpath:''}`);
      if(flavor==='web') {
        const bytes=await readFile(new URL(`../dist/${variant?variant+'/':''}web/bin_packing_wasm_bg.wasm`,import.meta.url));
        const raw=await api.default({module_or_path:bytes});
        assert.ok(raw.memory instanceof WebAssembly.Memory);
        for(const dimension of dimensions)assert.equal(typeof raw[`solve${dimension}d`],'function');
        const sync=await import(new URL(`../dist/${variant?variant+'/':''}web/bin_packing_wasm.js?sync`,import.meta.url));
        assert.ok(sync.initSync({module:bytes}).memory instanceof WebAssembly.Memory);
        packageContract(sync,dimensions);
      }
      for(const dimension of [1,2,3]) {
        assert.equal(typeof api[`solve${dimension}d`],dimensions.includes(dimension)?'function':'undefined');
        assert.equal(typeof api[`solve${dimension}dJson`],variant?'undefined':'function');
      }
      packageContract(api,dimensions);
    });
  }
}
