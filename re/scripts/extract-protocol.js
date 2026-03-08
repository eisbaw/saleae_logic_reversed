const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');

// Extract larger context around getMessageContent
const idx = code.indexOf('getMessageContent');
if (idx >= 0) {
  // Find the function boundary
  const start = Math.max(0, idx - 200);
  const end = Math.min(code.length, idx + 3000);
  console.log("=== getMessageContent full context ===");
  console.log(code.substring(start, end));
  console.log("\n\n");
}

// Search for getBackendConfig implementation
const idx2 = code.indexOf('getBackendConfig');
if (idx2 >= 0) {
  const start = Math.max(0, idx2 - 200);
  const end = Math.min(code.length, idx2 + 2000);
  console.log("=== getBackendConfig full context ===");
  console.log(code.substring(start, end));
  console.log("\n\n");
}

// Search for "request" type in message context (as sent to backend)
const requestPattern = /type:\s*["']request["']/g;
let m;
while ((m = requestPattern.exec(code)) !== null) {
  const start = Math.max(0, m.index - 300);
  const end = Math.min(code.length, m.index + 300);
  console.log("=== request type context ===");
  console.log(code.substring(start, end));
  console.log("\n");
}

// Search for sendRequest or sendMessage patterns
for (const term of ['sendRequest', 'sendMessage', 'backendSend', 'sendToBackend']) {
  let searchIdx = 0;
  let count = 0;
  while ((searchIdx = code.indexOf(term, searchIdx)) >= 0 && count < 5) {
    const start = Math.max(0, searchIdx - 200);
    const end = Math.min(code.length, searchIdx + 500);
    console.log(`=== ${term} context ===`);
    console.log(code.substring(start, end));
    console.log("\n");
    searchIdx += term.length;
    count++;
  }
}
