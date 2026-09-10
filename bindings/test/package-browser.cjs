// Run with an installed Playwright module path, or resolve Playwright normally.
const {createServer}=require('node:http');
const {readFile}=require('node:fs/promises');
const path=require('node:path');
async function main() {
  const {chromium}=require(process.argv[2]||'playwright');
  const routes={'/':null};
  for(const variant of ['combined','one-d','two-d','three-d'])for(const file of ['bin_packing_wasm.js','bin_packing_wasm_bg.wasm'])
    routes[`/${variant}/${file}`]=path.join(__dirname,`../wasm/dist/${variant==='combined'?'':variant+'/'}web/${file}`);
  const server=createServer(async(req,res)=>{
    try {
      if(!(req.url in routes)){res.writeHead(404);res.end();return;}
      res.setHeader('Content-Type',req.url.endsWith('.wasm')?'application/wasm':req.url==='/'?'text/html':'text/javascript');
      res.end(routes[req.url]?await readFile(routes[req.url]):'<!doctype html><title>Package contract</title>');
    } catch(error){res.writeHead(500);res.end(String(error));}
  });
  let browser;
  try {
    await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
    browser=await chromium.launch({headless:true});const page=await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}`);
    await page.addScriptTag({content:await readFile(path.join(__dirname,'package-contract.cjs'),'utf8')});
    const results=[];
    for(const [variant,dimensions]of [['combined',[1,2,3]],['one-d',[1]],['two-d',[2]],['three-d',[3]]]) {
      results.push(await page.evaluate(async({variant,dimensions})=>{
        const api=await import(`/${variant}/bin_packing_wasm.js`);await api.default();
        packageContract(api,dimensions);return {variant,dimensions,result:'passed'};
      },{variant,dimensions}));
    }
    console.log(JSON.stringify({browser:await browser.version(),results},null,2));
  } finally {
    if(browser)await browser.close();
    await new Promise((resolve,reject)=>server.close(error=>error?reject(error):resolve()));
  }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
