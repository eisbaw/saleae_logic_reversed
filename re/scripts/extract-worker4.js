const fs = require('fs');
const code = fs.readFileSync('re/extracted-asar/dist/logic/6a4322176d988210675b.worker.js', 'utf-8');

// Find loadDirect full implementation
console.log("=== loadDirect full ===");
const idx = code.indexOf('async loadDirect');
if (idx >= 0) {
  const end = Math.min(code.length, idx + 2000);
  console.log(code.substring(idx, end));
}

// Find flushLogs
console.log("\n=== flushLogs ===");
const flIdx = code.indexOf('async flushLogs');
if (flIdx >= 0) {
  const start = Math.max(0, flIdx - 200);
  const end = Math.min(code.length, flIdx + 500);
  console.log(code.substring(start, end));
}

// Find node types (for graph configuration)
console.log("\n=== Node type constants ===");
const ntPattern = /[A-Z_]+_NODE[A-Z_]*\s*=\s*[0-9]+/g;
let m;
while ((m = ntPattern.exec(code)) !== null) {
  console.log("  " + m[0]);
}

// Find node ID constants
console.log("\n=== Node ID definitions ===");
for (const term of ['LOGIC_DEVICE_NODE', 'DISAGGREGATOR_NODE', 'DEVICE_LIST_NODE', 'DEVICE_LIST_GRAPH']) {
  const termIdx = code.indexOf(term);
  if (termIdx >= 0) {
    const start = Math.max(0, termIdx - 100);
    const end = Math.min(code.length, termIdx + 200);
    console.log(code.substring(start, end));
    console.log("");
  }
}

// Find graphConfigurations
console.log("\n=== graphConfigurations ===");
const gcIdx = code.indexOf('graphConfigurations');
if (gcIdx >= 0) {
  const start = Math.max(0, gcIdx - 100);
  const end = Math.min(code.length, gcIdx + 3000);
  console.log(code.substring(start, end));
}

// Find the automation gRPC server code
console.log("\n=== gRPC / automation ===");
const grpcIdx = code.indexOf('grpc');
if (grpcIdx >= 0) {
  const start = Math.max(0, grpcIdx - 200);
  const end = Math.min(code.length, grpcIdx + 500);
  console.log(code.substring(start, end));
}
