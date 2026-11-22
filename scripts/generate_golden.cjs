// Developer-only: compare the Rust WASM bridge to the vendor's original JS glue.
// Usage: node scripts/generate_golden.cjs /path/to/sdk/dist
const fs = require('fs');
const path = require('path');
const vm = require('vm');
(async () => {
  const dist = process.argv[2];
  if (!dist) throw new Error('Pass original SDK dist path');
  const wasm = fs.readFileSync(path.join(dist, 'Nemonic.wasm'));
  const context = vm.createContext({ WebAssembly, Uint8Array, Uint8ClampedArray, ArrayBuffer,
    Response, self: undefined, console });
  vm.runInContext(fs.readFileSync(path.join(dist, 'nemonic_wasm.js'), 'utf8'), context, { timeout: 5000 });
  const api = await context.NemonicWasm.create({ wasmUrl: 'local.wasm',
    fetchImpl: async () => new Response(wasm, { headers: { 'Content-Type': 'application/wasm' } }) });
  const specs = [
    ['white',8,1,1,false], ['black',8,2,1,false], ['odd',13,7,3,false],
    ['gradient',64,32,2,false], ['dither',64,32,2,true], ['alpha',17,11,255,true],
    ['tall',8,2001,1,false], ['normal',576,96,1,true],
  ];
  const cases = [];
  for (const [name,width,height,copies,dither] of specs) {
    const rgba = new Uint8Array(width*height*4);
    for(let i=0;i<rgba.length;i+=4) {
      rgba[i] = name === 'white' ? 255 : name === 'black' ? 0 : (i*13+37)%256;
      rgba[i+1] = name === 'white' ? 255 : name === 'black' ? 0 : (i*7+53)%256;
      rgba[i+2] = name === 'white' ? 255 : name === 'black' ? 0 : (i*3+97)%256;
      rgba[i+3] = name === 'alpha' ? (i*11)%256 : 255;
    }
    // Construct the byte view in the vendor glue's VM realm.
    context.inputBytes = Array.from(rgba);
    const guest = vm.runInContext('new Uint8Array(inputBytes)', context);
    const result = api.buildPrintCommand(width,height,guest,copies,dither);
    fs.writeFileSync(`tests/fixtures/${name}.rgba`,rgba);
    fs.writeFileSync(`tests/fixtures/${name}.bin`,result);
    console.log(name, width,height,copies,dither,result.length,Buffer.from(result).subarray(0,28).toString('hex'));
    cases.push({name,width,height,copies,dither,bytes:result.length});
  }
  fs.writeFileSync('tests/fixtures/cases.json',JSON.stringify(cases,null,2)+'\n');
})().catch(e => { console.error(e); process.exitCode=1; });
