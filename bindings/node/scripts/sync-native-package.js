'use strict';

const fs = require('node:fs');
const path = require('node:path');

const rootDir = path.resolve(__dirname, '..');
const binaryStem = 'bin-packing';
const sourceBinary = path.join(rootDir, `${binaryStem}.darwin-arm64.node`);
const targetDir = path.join(rootDir, 'npm', 'darwin-arm64');
const targetBinary = path.join(targetDir, `${binaryStem}.darwin-arm64.node`);

if (!fs.existsSync(sourceBinary)) {
  throw new Error(`Expected native binary at ${sourceBinary}`);
}

fs.mkdirSync(targetDir, { recursive: true });
fs.copyFileSync(sourceBinary, targetBinary);
