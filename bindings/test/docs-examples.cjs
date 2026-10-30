// Execute marked documentation examples against the actual native/WASM packages.
'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const files = ['README.md', 'bindings/node/README.md', 'bindings/wasm/README.md', 'docs/bindings.md'];
const examples = files.flatMap(file => {
  const text = fs.readFileSync(path.join(root, file), 'utf8');
  const blocks = [...text.matchAll(/<!-- bin-packing:example -->\s*```js\n([\s\S]*?)^```/gm)];
  const markers = text.match(/<!-- bin-packing:example -->/g) ?? [];
  assert.equal(blocks.length, markers.length, `${file}: each example marker needs a JS block`);
  return blocks.map((match, index) => ({ file, index, source: match[1] }));
});
assert.ok(examples.length > 0, 'documentation must contain runnable examples');

for (const backend of process.argv.slice(2).length ? process.argv.slice(2) : ['native', 'wasm']) {
  assert.ok(['native', 'wasm'].includes(backend), `unsupported backend: ${backend}`);
  const api = backend === 'native' ? require('../node') : require('../wasm/dist/nodejs/bin_packing_wasm.cjs');
  const exampleRequire = name => ['@0xdoublesharp/bin-packing', '@0xdoublesharp/bin-packing-wasm/nodejs'].includes(name) ? api : require(name);
  for (const example of examples) {
    try {
      new Function('require', example.source)(exampleRequire);
    } catch (error) {
      throw new Error(`${backend}: ${example.file} example ${example.index} failed`, { cause: error });
    }
  }
  console.log(`${backend}: ${examples.length} documentation examples passed.`);
}
