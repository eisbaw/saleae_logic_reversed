const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/6a4322176d988210675b.worker.js', 'utf-8');

// Search for messageType handler dispatch
console.log("=== messageType handler dispatch ===");
const messageTypePattern = /messageType/g;
let m;
let count = 0;
while ((m = messageTypePattern.exec(code)) !== null && count < 20) {
  const start = Math.max(0, m.index - 200);
  const end = Math.min(code.length, m.index + 500);
  console.log(code.substring(start, end));
  console.log("\n---\n");
  count++;
}
