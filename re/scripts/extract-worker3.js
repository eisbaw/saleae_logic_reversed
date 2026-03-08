const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/6a4322176d988210675b.worker.js', 'utf-8');

// Extract all Saleae::* type strings
console.log("=== All Saleae:: message types ===");
const saleaePattern = /Saleae::[A-Za-z:]+/g;
const saleaeTypes = new Set();
let m;
while ((m = saleaePattern.exec(code)) !== null) {
  saleaeTypes.add(m[0]);
}
for (const t of [...saleaeTypes].sort()) console.log("  " + t);

// Find GraphSocket loadDirect implementation
console.log("\n=== loadDirect implementation ===");
const ldIdx = code.indexOf('loadDirect');
if (ldIdx >= 0) {
  const start = Math.max(0, ldIdx - 200);
  const end = Math.min(code.length, ldIdx + 3000);
  console.log(code.substring(start, end));
}

// Find handleMessage
console.log("\n=== handleMessage ===");
const hmIdx = code.indexOf('handleMessage');
if (hmIdx >= 0) {
  const start = Math.max(0, hmIdx - 200);
  const end = Math.min(code.length, hmIdx + 3000);
  console.log(code.substring(start, end));
}

// Find sendLogicDeviceRequest
console.log("\n=== sendLogicDeviceRequest ===");
const sldrIdx = code.indexOf('sendLogicDeviceRequest');
if (sldrIdx >= 0) {
  const start = Math.max(0, sldrIdx - 200);
  const end = Math.min(code.length, sldrIdx + 2000);
  console.log(code.substring(start, end));
}

// Find graphActions
console.log("\n=== graphActions definitions ===");
const gaIdx = code.indexOf('graphActions');
if (gaIdx >= 0) {
  const start = Math.max(0, gaIdx - 100);
  const end = Math.min(code.length, gaIdx + 2000);
  console.log(code.substring(start, end));
}
