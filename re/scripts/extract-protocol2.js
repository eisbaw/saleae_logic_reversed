const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');

// Find graphClientInterface and its methods
console.log("=== graphClientInterface connect context ===");
let idx = code.indexOf('graphClientInterface');
let count = 0;
while (idx >= 0 && count < 3) {
  const start = Math.max(0, idx - 100);
  const end = Math.min(code.length, idx + 500);
  console.log(code.substring(start, end));
  console.log("\n---\n");
  idx = code.indexOf('graphClientInterface', idx + 1);
  count++;
}

// Search for ".connect(" pattern to find the connection setup
console.log("=== connect method implementations ===");
const connectPattern = /\.connect\(\{mode:/g;
let m;
while ((m = connectPattern.exec(code)) !== null) {
  const start = Math.max(0, m.index - 300);
  const end = Math.min(code.length, m.index + 500);
  console.log(code.substring(start, end));
  console.log("\n---\n");
}

// Search for "saleae" WebSocket endpoint
console.log("=== /saleae WebSocket endpoint ===");
idx = code.indexOf('/saleae');
count = 0;
while (idx >= 0 && count < 3) {
  const start = Math.max(0, idx - 200);
  const end = Math.min(code.length, idx + 500);
  console.log(code.substring(start, end));
  console.log("\n---\n");
  idx = code.indexOf('/saleae', idx + 1);
  count++;
}

// Search for Request types being constructed and sent to backend
console.log("=== Backend request types ===");
for (const term of ['"request"', "'request'", 'type:"request"']) {
  let searchIdx = 0;
  let found = 0;
  while ((searchIdx = code.indexOf(term, searchIdx)) >= 0 && found < 5) {
    const start = Math.max(0, searchIdx - 200);
    const end = Math.min(code.length, searchIdx + 400);
    console.log(`--- "${term}" context ---`);
    console.log(code.substring(start, end));
    console.log("\n");
    searchIdx += term.length;
    found++;
  }
}
