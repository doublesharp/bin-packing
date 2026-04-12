#!/usr/bin/env node
// Build the WASM package for bundler, web, and Node.js targets.
//
// `wasm-pack build` can only target one flavor at a time, so we fan out here
// and then normalize each target's output directory:
//
// 1. Delete the per-target package.json / README / LICENSE / .gitignore that
//    wasm-pack generates. We publish a single root package.json that
//    re-exports all three flavors via the `exports` map, not one per dir.
// 2. Overwrite wasm-bindgen's `any`-typed `.d.ts` with our hand-written one
//    that provides full IntelliSense on problems, options, and solutions.

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { readFile, rm, readdir, unlink, stat, writeFile, rename } from 'node:fs/promises';

const here = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(here, '..');

const variants = [
  {
    name: 'full',
    distPrefix: 'dist',
    features: ['one-d', 'two-d', 'three-d', 'json'],
    types: 'types/bin_packing_wasm.d.ts',
  },
  {
    name: 'one-d',
    distPrefix: 'dist/one-d',
    features: ['one-d'],
    types: 'types/one_d.d.ts',
    webInit: 'types/web-init-one-d.d.ts',
  },
  {
    name: 'two-d',
    distPrefix: 'dist/two-d',
    features: ['two-d'],
    types: 'types/two_d.d.ts',
    webInit: 'types/web-init-two-d.d.ts',
  },
  {
    name: 'three-d',
    distPrefix: 'dist/three-d',
    features: ['three-d'],
    types: 'types/three_d.d.ts',
    webInit: 'types/web-init-three-d.d.ts',
  },
];

const targetFlavors = [
  { flag: 'bundler', subdir: 'bundler', hasInit: false, renameToCjs: false },
  { flag: 'web', subdir: 'web', hasInit: true, renameToCjs: false },
  // wasm-pack's `nodejs` target emits CommonJS. Since the package declares
  // `"type": "module"`, we rename the emitted `.js` to `.cjs` so Node.js
  // picks the right loader regardless of consumer tsconfig/package settings.
  { flag: 'nodejs', subdir: 'nodejs', hasInit: false, renameToCjs: true },
];

function run(command, args, cwd) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      cwd,
      stdio: 'inherit',
      env: process.env,
    });
    child.on('error', rejectPromise);
    child.on('exit', (code) => {
      if (code === 0) resolvePromise();
      else rejectPromise(new Error(`${command} ${args.join(' ')} exited with code ${code}`));
    });
  });
}

async function removeIfExists(path) {
  try {
    await stat(path);
    await unlink(path);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
}

async function dropPerTargetMetadata(outDir) {
  const outPath = resolve(packageDir, outDir);
  let entries;
  try {
    entries = await readdir(outPath);
  } catch (error) {
    if (error.code === 'ENOENT') return;
    throw error;
  }
  for (const entry of ['package.json', '.gitignore', 'README.md', 'LICENSE']) {
    if (entries.includes(entry)) {
      await removeIfExists(resolve(outPath, entry));
    }
  }
}

async function overrideTypes(outDir, hasInit, renameToCjs, typesPath, webInitPath) {
  const outPath = resolve(packageDir, outDir);
  const typedPath = resolve(packageDir, typesPath);
  let content = await readFile(typedPath, 'utf8');
  if (hasInit) {
    const initPath = resolve(packageDir, webInitPath ?? 'types/web-init.d.ts');
    const initContent = await readFile(initPath, 'utf8');
    content = `${content}\n\n${initContent}`;
  }
  const typesFileName = renameToCjs ? 'bin_packing_wasm.d.cts' : 'bin_packing_wasm.d.ts';
  await writeFile(resolve(outPath, typesFileName), content);
  if (renameToCjs) {
    // Drop the `.d.ts` wasm-pack wrote; we only ship `.d.cts` for the CJS target.
    await removeIfExists(resolve(outPath, 'bin_packing_wasm.d.ts'));
  }
}

async function renameJsToCjs(outDir) {
  const outPath = resolve(packageDir, outDir);
  const jsPath = resolve(outPath, 'bin_packing_wasm.js');
  const cjsPath = resolve(outPath, 'bin_packing_wasm.cjs');
  await rename(jsPath, cjsPath);

  // wasm-pack's nodejs `.js` file has a self-types directive pointing at
  // `./bin_packing_wasm.d.ts`. Our override writes `.d.cts` instead, so
  // patch the directive to match.
  const contents = await readFile(cjsPath, 'utf8');
  const patched = contents.replace(
    '/* @ts-self-types="./bin_packing_wasm.d.ts" */',
    '/* @ts-self-types="./bin_packing_wasm.d.cts" */',
  );
  if (patched === contents) {
    throw new Error(
      `renameJsToCjs: @ts-self-types patch had no effect in ${cjsPath} — wasm-pack output format may have changed`,
    );
  }
  await writeFile(cjsPath, patched);
}

async function main() {
  await rm(resolve(packageDir, 'dist'), { recursive: true, force: true });

  for (const variant of variants) {
    for (const target of targetFlavors) {
      const outDir = `${variant.distPrefix}/${target.subdir}`;
      console.log(`\n== ${variant.name}: wasm-pack build --target ${target.flag} ==`);
      await run(
        'wasm-pack',
        [
          'build',
          '--release',
          '--target',
          target.flag,
          '--out-dir',
          outDir,
          '--out-name',
          'bin_packing_wasm',
          '--',
          '--no-default-features',
          '--features',
          variant.features.join(','),
        ],
        packageDir,
      );
      await dropPerTargetMetadata(outDir);
      await overrideTypes(outDir, target.hasInit, target.renameToCjs, variant.types, variant.webInit);
      if (target.renameToCjs) {
        await renameJsToCjs(outDir);
      }
    }
  }

  console.log('\nAll targets built. Output in dist/{bundler,web,nodejs} and dist/{one-d,two-d,three-d}/{bundler,web,nodejs}/.');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
