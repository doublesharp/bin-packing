// Run: node bindings/test/saw-performance-browser.cjs /path/to/node_modules/playwright
const { createServer } = require('node:http');
const { readFile } = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require(process.argv[2] || 'playwright');

async function main() {
  const server = createServer(async (req, res) => {
    try {
      const routes = {
        '/': [null, 'text/html'],
        '/combined/bin_packing_wasm.js': ['../wasm/dist/web/bin_packing_wasm.js', 'text/javascript'],
        '/combined/bin_packing_wasm_bg.wasm': ['../wasm/dist/web/bin_packing_wasm_bg.wasm', 'application/wasm'],
        '/two-d/bin_packing_wasm.js': ['../wasm/dist/two-d/web/bin_packing_wasm.js', 'text/javascript'],
        '/two-d/bin_packing_wasm_bg.wasm': ['../wasm/dist/two-d/web/bin_packing_wasm_bg.wasm', 'application/wasm'],
      };
      const route = routes[req.url];
      if (!route) { res.writeHead(404); res.end(); return; }
      res.setHeader('Content-Type', route[1]);
      res.end(route[0] ? await readFile(path.join(__dirname, route[0])) : '<!doctype html><title>Saw planner WASM benchmark</title>');
    } catch (error) { res.writeHead(500); res.end(String(error)); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  let browser;
  try {
    browser = await chromium.launch({ headless: true });
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.addScriptTag({ content: await readFile(path.join(__dirname, 'saw-replay.cjs'), 'utf8') });
    await page.addScriptTag({ content: await readFile(path.join(__dirname, 'saw-performance.cjs'), 'utf8') });
    const results = { browser: await browser.version(), user_agent: await page.evaluate(() => navigator.userAgent) };
    for (const variant of ['combined', 'two-d']) {
      results[variant] = await page.evaluate(async variant => {
        const api = await import(`/${variant}/bin_packing_wasm.js`);
        const initialized = await api.default();
        if (typeof initialized.plan2dSawCuts !== 'function') throw new Error('missing raw saw export');
        return {grids:measure(api),strips:measureStrips(api)};
      }, variant);
    }
    console.log(JSON.stringify(results, null, 2));
  } finally {
    if (browser) await browser.close();
    await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
