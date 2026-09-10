// Package scripts use the saved build workspace automatically when configured.
const {existsSync} = require('node:fs');
const {dirname, relative, resolve} = require('node:path');
const {spawnSync} = require('node:child_process');
const root = resolve(__dirname, '..');
const cwd = process.cwd();
const [tool, ...args] = process.argv.slice(2);
let command, parameters;
if (existsSync(resolve(root, '.scratch.json'))) {
  command = process.env.BIN_PACKING_PYTHON || 'python3';
  parameters = [resolve(root, 'scripts/scratch.py'), 'run', '--cwd', relative(root, cwd),
    process.execPath, relative(cwd, __filename), tool, ...args];
} else if (tool === 'node') {
  command = process.execPath;
  parameters = args;
} else if (tool === 'napi') {
  const manifest = require.resolve('@napi-rs/cli/package.json', {paths: [cwd]});
  command = process.execPath;
  parameters = [resolve(dirname(manifest), require(manifest).bin.napi), ...args];
} else {
  throw new Error(`Unsupported package tool: ${tool}`);
}
const child = spawnSync(command, parameters, {cwd, stdio: 'inherit', env: process.env});
if (child.error) {
  console.error(child.error.message);
  process.exitCode = 1;
} else if (child.signal) {
  process.kill(process.pid, child.signal);
} else {
  process.exitCode = child.status;
}
