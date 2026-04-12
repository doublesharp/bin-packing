'use strict';

const path = require('node:path');

const loadErrors = [];

function loadFrom(requirePath) {
  try {
    return require(requirePath);
  } catch (error) {
    loadErrors.push(error);
    return null;
  }
}

function loadNative() {
  if (process.platform === 'darwin' && process.arch === 'arm64') {
    const localBinary = loadFrom(path.join(__dirname, 'npm', 'darwin-arm64', 'bin-packing.darwin-arm64.node'));
    if (localBinary) {
      return localBinary;
    }

    const packageBinary = loadFrom('@0xdoublesharp/bin-packing-darwin-arm64');
    if (packageBinary) {
      return packageBinary;
    }
  }

  const details = loadErrors.map((error) => `- ${error.message}`).join('\n');
  const supportedTarget = 'darwin-arm64';
  throw new Error(
    [
      `Unable to load the 0xdoublesharp/bin-packing native binding for ${process.platform}-${process.arch}.`,
      `This local packaging setup currently provides ${supportedTarget} only.`,
      details && 'Load failures:',
      details
    ]
      .filter(Boolean)
      .join('\n')
  );
}

module.exports = loadNative();
