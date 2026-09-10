// Build bindings first, then run: node bindings/test/saw-performance-node.cjs
const { measure, measureStrips } = require('./saw-performance.cjs');
const os = require('node:os');
const result = { environment: { node: process.version, platform: process.platform, arch: process.arch, cpu: os.cpus()[0].model },
  native: measure(require('../node/wrapper.js')),
  wasm_combined: measure(require('../wasm/dist/nodejs/bin_packing_wasm.cjs')),
  wasm_two_d: measure(require('../wasm/dist/two-d/nodejs/bin_packing_wasm.cjs')) };
result.strips={native:measureStrips(require('../node/wrapper.js')),wasm_combined:measureStrips(require('../wasm/dist/nodejs/bin_packing_wasm.cjs')),wasm_two_d:measureStrips(require('../wasm/dist/two-d/nodejs/bin_packing_wasm.cjs'))};
console.log(JSON.stringify(result, null, 2));
