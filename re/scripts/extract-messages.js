const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/bundle.js', 'utf-8');

// Search for request_type patterns
const patterns = [
  /request_type:\s*"[^"]+"/g,
  /requestType:\s*"[^"]+"/g,
  /request_type\s*:\s*'[^']+'/g,
];

const found = new Set();
for (const pat of patterns) {
  let m;
  while ((m = pat.exec(code)) !== null) {
    found.add(m[0]);
  }
}
console.log("=== Request Type Patterns ===");
const sorted = [...found].sort();
for (const s of sorted) console.log(s);

// Also search for message structure patterns
console.log("\n=== Send patterns ===");
const sendPattern = /\.send\(\{[^}]{0,200}\}/g;
const sendMatches = new Set();
let m2;
while ((m2 = sendPattern.exec(code)) !== null) {
  sendMatches.add(m2[0]);
}
for (const s of [...sendMatches].sort()) console.log(s);
