const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/main.js', 'utf-8');

// Find the FFI function definitions (CreateGraphServer, etc.)
console.log("=== FFI Library Definition ===");
const ffiIdx = code.indexOf('CreateGraphServer');
if (ffiIdx >= 0) {
  const start = Math.max(0, ffiIdx - 500);
  const end = Math.min(code.length, ffiIdx + 1500);
  console.log(code.substring(start, end));
}

// Find needsOutOfProcessBackend
console.log("\n=== needsOutOfProcessBackend usage ===");
const noopbIdx = code.indexOf('needsOutOfProcessBackend');
if (noopbIdx >= 0) {
  const start = Math.max(0, noopbIdx - 500);
  const end = Math.min(code.length, noopbIdx + 500);
  console.log(code.substring(start, end));
}

// Find the WebSocket part of the out-of-process mode
console.log("\n=== Out-of-process mode ===");
const oopIdx = code.indexOf('logic_websocket');
if (oopIdx >= 0) {
  const start = Math.max(0, oopIdx - 1000);
  const end = Math.min(code.length, oopIdx + 1000);
  console.log(code.substring(start, end));
}

// Search for automation port
console.log("\n=== Automation API ===");
const autoIdx = code.indexOf('automationPort');
if (autoIdx >= 0) {
  const start = Math.max(0, autoIdx - 300);
  const end = Math.min(code.length, autoIdx + 300);
  console.log(code.substring(start, end));
}

// Search for protobuf in the worker bundle
const workerCode = fs.readFileSync('re/extracted-asar/dist/logic/6a4322176d988210675b.worker.js', 'utf-8');
console.log("\n=== Protobuf in worker ===");
const pbIdx = workerCode.indexOf('proto');
if (pbIdx >= 0) {
  const start = Math.max(0, pbIdx - 200);
  const end = Math.min(workerCode.length, pbIdx + 500);
  console.log(workerCode.substring(start, end));
}

// Search for automation proto definitions in the main bundle
const bundleCode = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');
console.log("\n=== Proto/Automation in renderer bundle ===");
const protoPatterns = [/proto\./g, /\.proto/g, /protobuf/gi, /proto3/gi, /saleae\.automation/gi];
for (const pat of protoPatterns) {
  let m;
  let c = 0;
  while ((m = pat.exec(bundleCode)) !== null && c < 3) {
    const start = Math.max(0, m.index - 100);
    const end = Math.min(bundleCode.length, m.index + 200);
    console.log(`--- Pattern: ${pat.source} ---`);
    console.log(bundleCode.substring(start, end));
    console.log("");
    c++;
  }
}
