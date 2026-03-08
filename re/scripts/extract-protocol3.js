const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');

// Find GraphClientInterface class
console.log("=== GraphClientInterface ===");
let idx = code.indexOf('GraphClientInterface');
let count = 0;
while (idx >= 0 && count < 5) {
  const start = Math.max(0, idx - 200);
  const end = Math.min(code.length, idx + 2000);
  console.log(code.substring(start, end));
  console.log("\n===\n");
  idx = code.indexOf('GraphClientInterface', idx + 1);
  count++;
}

// Find the actual connect implementation (WebSocket mode)
console.log("\n=== WebSocket connect ===");
idx = code.indexOf('new WebSocket');
count = 0;
while (idx >= 0 && count < 3) {
  const start = Math.max(0, idx - 500);
  const end = Math.min(code.length, idx + 1500);
  console.log(code.substring(start, end));
  console.log("\n===\n");
  idx = code.indexOf('new WebSocket', idx + 1);
  count++;
}
