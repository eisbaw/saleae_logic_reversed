const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');

// Extract all messageType values
console.log("=== All messageType values ===");
const messageTypePattern = /messageType:\s*"([^"]+)"/g;
const messageTypes = new Set();
let m;
while ((m = messageTypePattern.exec(code)) !== null) {
  messageTypes.add(m[1]);
}
for (const t of [...messageTypes].sort()) console.log("  " + t);

// Find the full GraphClientInterface class - extract more context
console.log("\n=== GraphClientInterface full class ===");
const classStart = code.indexOf('e.GraphClientInterface=class{constructor');
if (classStart >= 0) {
  // Try to find the class end by counting braces
  let depth = 0;
  let pos = classStart;
  let started = false;
  let classCode = '';
  for (; pos < code.length && pos < classStart + 20000; pos++) {
    if (code[pos] === '{') { depth++; started = true; }
    if (code[pos] === '}') depth--;
    classCode += code[pos];
    if (started && depth === 0) break;
  }
  console.log(classCode.substring(0, 10000));
}

// Look for the worker that handles messages
console.log("\n\n=== Worker message handlers ===");
const idx = code.indexOf('6a4322176d988210675b.worker.js');
if (idx >= 0) {
  const start = Math.max(0, idx - 500);
  const end = Math.min(code.length, idx + 1000);
  console.log(code.substring(start, end));
}

// Search for "postMessage" patterns
console.log("\n=== postMessage implementations ===");
const pmIdx = code.indexOf('postMessage({messageType');
if (pmIdx >= 0) {
  const start = Math.max(0, pmIdx - 500);
  const end = Math.min(code.length, pmIdx + 500);
  console.log(code.substring(start, end));
}
