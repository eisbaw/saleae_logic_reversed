const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/6a4322176d988210675b.worker.js', 'utf-8');

// Find messageHandlers registration
console.log("=== messageHandlers registration ===");
const idx = code.indexOf('messageHandlers');
let count = 0;
while (idx >= 0 && count < 3) {
  const start = Math.max(0, code.indexOf('messageHandlers', count > 0 ? idx + 1 : 0) - 100);
  const end = Math.min(code.length, start + 5000);
  console.log(code.substring(start, end));
  console.log("\n===END===\n");
  count++;
  break;  // Get the first (longest) context
}

// Search for GraphSocket class
console.log("\n=== GraphSocket class ===");
let gsIdx = code.indexOf('GraphSocket');
if (gsIdx >= 0) {
  const start = Math.max(0, gsIdx - 200);
  const end = Math.min(code.length, gsIdx + 5000);
  console.log(code.substring(start, end));
}
