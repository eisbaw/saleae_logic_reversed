const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');

// Extract all proto.saleae.automation.* symbols
console.log("=== All proto.saleae.automation.* symbols ===");
const protoPattern = /proto\.saleae\.automation\.([A-Za-z_.]+)/g;
const protos = new Set();
let m;
while ((m = protoPattern.exec(code)) !== null) {
  protos.add(m[1]);
}
for (const p of [...protos].sort()) console.log("  " + p);

// Find the automation port/service definition
console.log("\n=== Automation service ===");
for (const term of ['AutomationService', 'automation_service', 'ManagementService', 'saleae.automation']) {
  let idx = code.indexOf(term);
  if (idx >= 0) {
    const start = Math.max(0, idx - 100);
    const end = Math.min(code.length, idx + 500);
    console.log(`--- ${term} ---`);
    console.log(code.substring(start, end));
    console.log("\n");
  }
}
