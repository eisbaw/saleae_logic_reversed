# Export Node / Exporter Analysis

Source path: `/workspace/monorepo/graph-io/nodes/export_node/src/exporter.cpp`

## Overview

The Saleae Logic 2 export system has two distinct export paths:

1. **Raw Data Export** (ExportNode / Exporter) -- exports waveform data (digital+analog) to CSV or binary files
2. **Analyzer Export** (AnalyzerExportNode) -- exports protocol analyzer results (decoded frames)

There is also a third mechanism:
3. **WaveformStoreNode::ExportWaveformCSV** -- currently throws "not implemented" (stub at ELF 0x2c8a570)

This document focuses on path 1 (raw data export), which is the primary user-facing export.

## Architecture

```
Electron UI (bundle.js)
  |  exportActions.exportData(M)
  v
ExportNode (Saleae::Graph::ExportNode)
  |  HandleExportDataAction()
  v
Exporter (Saleae::Graph::Exporter)
  |  RunExportOnThread()
  |  Creates output directory, opens files, writes data
  v
Digital/Analog Store APIs
  |  Reads from WaveformStoreNode data
  v
Output files (CSV or Binary)
```

## Key Classes and Functions

| Class / Function | ELF VA | Purpose |
|---|---|---|
| ExportNode constructor | 0x1af9bb0 | Initializes export node, registers handlers |
| ExportNode::HandleExportDataAction | 0x1afa1d0 | Validates channels, creates Exporter, starts thread |
| Exporter::RunExportOnThread | 0x1aff5e0 | Main export loop: creates dirs, opens files, writes data |
| ExportNodeState deserializer | 0x1afe020 | Deserializes export state from JSON |
| ExportNodeState constructor | 0x1afdda0 | Creates default ExportNodeState |
| Exporter::GetExportPaths | 0x1b03ae0 | Returns file paths (throws if export still running) |
| File open helper | 0x1b03b50 | Opens output file, handles errors |
| ExportWaveformCSV (stub) | 0x2c8a570 | Throws "not implemented" |

## Export State Machine

The export state is tracked as an enum (string-serialized):

| Value | Numeric | Meaning |
|---|---|---|
| ExportNotReady | 0 | No export possible (no data, or still capturing) |
| ExportReady | 1 | Data available, export can be started |
| Exporting | 2 | Export in progress |

Parsed at ELF 0x2ca2900 -- character-by-character string comparison.

## ExportData Action Fields

The `Saleae::Graph::ExportActions::ExportData` action (sent from the UI) contains these fields
(reconstructed from bundle.js and binary analysis):

```javascript
{
  exportFilepath: string,       // Output directory path
  graphInterval: {begin, end},  // Time range to export (graph time units)
  digitalChannels: [...],       // Array of digital channel descriptors
  analogChannels: [...],        // Array of analog channel descriptors
  analogDownsample: number,     // Analog downsample ratio (>= 1)
  format: "TextCsv" | "Binary", // Export format
  useWallClockTime: boolean,    // Use ISO 8601 wall-clock timestamps vs sample index
  zeroTime: number              // Reference time (capture start or base time)
}
```

The serialization fields map to JSON pointers used in the internal serialization system
(seen in the /exportXxx field paths).

## Export State Fields (ExportNodeState)

The ExportNodeState is reported back to the UI:

```
/exportState            -> "ExportNotReady" | "ExportReady" | "Exporting"
/exportProgress         -> boolean (export in progress)
/exportProgressPercentage -> double (0.0 to 1.0)
/exportFilepath         -> string (current output path)
/exportOptions          -> export configuration
/exportErrorMessage     -> string (error description if failed)
/exportIndex            -> integer (current export index)
```

## Export Formats

### 1. CSV Export (format = "TextCsv")

When format is "TextCsv", the exporter writes:

**Digital channels only (no analog):**
- Single file: `digital.csv` in the output directory
- Written by function at ~ELF 0x1b01730 (FUN_01c01730 in Ghidra)
- Contains timestamp + channel state columns

**Analog channels only (no digital):**
- Single file: `analog.csv` in the output directory
- Written by function at ~ELF 0x1b03410 (FUN_01c03410 in Ghidra)
- Contains timestamp + voltage columns
- Error on failure: "Failed while writing to analog.csv"

**Both digital and analog:**
- `digital.csv` is written first
- Then `analog.csv` is written
- Order is: digital first, then analog (see code flow at 0x1aff5e0)

### 2. Binary Export (format = "Binary")

When format is "Binary", the exporter writes:

**Digital channels:**
- One file per channel: `digital_N.bin` (N = 0, 1, 2, ...)
- Written by function at ~ELF 0x1b01280 (FUN_01c01280 in Ghidra)
- Channel index from loop counter (local_37c)
- File naming: `"digital_" + to_string(index) + ".bin"`

**Analog channels:**
- One file per channel: `analog_N.bin` (N = 0, 1, 2, ...)
- Written by function at ~ELF 0x1b03120 (FUN_01c03120 in Ghidra)
- Channel index from loop counter (local_37c)
- File naming: `"analog_" + to_string(index) + ".bin"`
- Additional parameter: downsample ratio from param_1 offset 0x70

## Export Flow (RunExportOnThread)

Reconstructed from decompilation of FUN_01bff5e0 (ELF 0x1aff5e0):

```
RunExportOnThread(Exporter* this):
  1. Log "Exporter" at level INFO
  2. Check output directory:
     - If doesn't exist: create_directories()
     - If exists but not a directory: error "Export path exists but is not a directory"
     - If creation fails: error "Failed to create directory"
  3. Branch on format (offset 0x38 in Exporter struct):
     if format == CSV (value 0):
       3a. Write digital.csv:
           - Construct filename "digital.csv"
           - Open file via helper (FUN_01c03b50)
           - Call digital CSV writer (FUN_01c01730) with:
             - digital channel data (offset 0x40)
             - cancel flag (offset 0xc0)
             - timestamp format flag (offset 0x74)
           - Close file
       3b. If analog channels present (offset 0x58 != offset 0x60):
           - Write analog.csv similarly (FUN_01c03410)
           - Parameters include downsample ratio

     if format == Binary (value != 0):
       3c. For each digital channel (iterate offset 0x40 vector):
           - Construct filename "digital_" + index + ".bin"
           - Open file
           - Call binary digital writer (FUN_01c01280)
           - Close file
       3d. For each analog channel (iterate offset 0x58 vector):
           - Construct filename "analog_" + index + ".bin"
           - Open file
           - Call binary analog writer (FUN_01c03120)
           - Close file

  4. If canceled (offset 0xc0 flag):
     - Log "Exporter was canceled, deleting files"
     - Delete all created files (from paths vector at offset 0xd0)
     - Set completion status

  5. Report progress (1.0 = 100%) via callback at offset 0xb0
  6. Cleanup file list
```

## HandleExportDataAction Flow

Reconstructed from decompilation of FUN_01bfa1d0 (ELF 0x1afa1d0):

```
HandleExportDataAction(ExportNode* this, ExportData& action):
  1. Check if export already in progress (offset 0x328):
     - If yes: throw "cannot start export while export in progress"

  2. Validate channels:
     - If no digital AND no analog channels: throw "at least 1 channel must be specified for export"

  3. For each digital channel in action:
     - Look up store at this + 0x228 + channel_index * 0x10
     - If store not set: throw "trying to export channel for digital api that has not been set"
     - Check store readiness: throw "cannot export data, a digital store is not ready"
     - Clip requested interval to available data range
     - Build channel export descriptor

  4. For each analog channel in action:
     - Look up store at this + 0x128 + channel_index * 0x10
     - If store not set: throw "trying to export channel for analog api that has not been set"
     - Check store readiness: throw "cannot export data, an analog store is not ready"
     - Check data availability: throw "cannot export data if some analog channels have no data"
     - Clip requested interval to available data range
     - Build channel export descriptor (includes sample rate info)

  5. Validate time range:
     - throw "failed to export: there is no available data to export" if empty

  6. Create Exporter object (0xf0 bytes, FUN_01bff2b0):
     - Pass: filepath, format, digital descriptors, analog descriptors,
       downsample ratio, graph interval, useWallClockTime flag, time offsets

  7. Submit to thread executor (offset 0x330)
  8. Store Exporter pointer at offset 0x328
```

## Exporter Struct Layout (partial)

Offset 0x38 in Exporter struct maps to format (0 = CSV, non-zero = Binary).

Relevant offsets in the Exporter object (size ~0xf0):
- 0x18: format flag (from ExportData action)
- 0x20: filepath (std::string -- export directory path)
- 0x38: format type (0 = CSV, 1 = Binary)
- 0x40: digital channels vector (begin pointer)
- 0x48: digital channels vector (end pointer)
- 0x58: analog channels vector (begin pointer)
- 0x60: analog channels vector (end pointer)
- 0x70: downsample ratio (int32)
- 0x74: useWallClockTime flag
- 0x78-0x88: graph interval / time range parameters
- 0x80: additional time parameter
- 0x88: additional format parameter
- 0x8c: flag byte
- 0x90-0xa0: three 8-byte values (time-related: zeroTime, etc.)
- 0xb0: progress callback (shared_ptr to function<void(bool, string, double)>)
- 0xc0: cancel flag (atomic, bit 0)
- 0xc4: completion status (int, set to 1 when done)
- 0xd0: created file paths vector (for cleanup on cancel)

## ExportNode Struct Layout (partial)

- 0x128 + N*0x10: analog store API pointers (per-channel, max 8 channels = 0x80 bytes)
- 0x228 + N*0x10: digital store API pointers (per-channel, max 16 channels = 0x100 bytes)
- 0x328: current Exporter pointer (null when idle)
- 0x330: thread executor (shared_ptr)
- 0x338: thread executor ref count

## Automation API (gRPC)

The Electron automation API exposes three export RPCs:

### ExportRawDataCsv
```
service Manager {
  rpc ExportRawDataCsv(ExportRawDataCsvRequest) returns (ExportRawDataCsvReply);
}

message ExportRawDataCsvRequest {
  uint64 capture_id = 1;
  string directory = 2;
  oneof channels {
    LogicChannels logic_channels = 3;
  }
  bool iso8601_timestamp = 4;
  uint32 analog_downsample_ratio = 5;
}
```

### ExportRawDataBinary
```
service Manager {
  rpc ExportRawDataBinary(ExportRawDataBinaryRequest) returns (ExportRawDataBinaryReply);
}

message ExportRawDataBinaryRequest {
  uint64 capture_id = 1;
  string directory = 2;
  oneof channels {
    LogicChannels logic_channels = 3;
  }
  uint32 analog_downsample_ratio = 4;
}
```

### ExportDataTableCsv (analyzer results)
```
message ExportDataTableCsvRequest {
  uint64 capture_id = 1;
  string filepath = 2;
  repeated DataTableAnalyzerConfiguration analyzers = 3;
  bool iso8601_timestamp = 4;
  repeated string export_columns = 5;
  DataTableFilter filter = 6;
}
```

## Waveform CSV Export (ExportWaveformCSV)

The `Saleae::Graph::StoreActions::ExportWaveformCSV` action targets the WaveformStoreNode
directly. Its fields are:

```javascript
{
  waveformId: number,
  timeRange: { begin: number, end: number },
  outputPath: string
}
```

Deserialization at ELF 0x2ca63f0 reads:
- `/outputPath` (string)
- `/waveformId` (uint64)
- `/timeRange` (begin/end pair)

However, the actual handler at 0x2c8a570 throws "ExportWaveformCSV not implemented".
This is a dead code path -- all real exports go through ExportNode.

## Analyzer Export (AnalyzerExportNode)

The AnalyzerExportNode (ELF 0x1c62cb0 for ExportThread) handles protocol analyzer exports.
It runs on a separate thread:

1. Calls `Detail::SetExportFlag()`
2. Gets `AnalyzerResults::GetAnalyzerResultsData()`
3. Sets progress callback via `AnalyzerResultsData::SetExportProgressCallback()`
4. Invokes the analyzer's export method
5. Logs "end of export thread"
6. Clears export-in-progress flag at offset 0xa40

## Capture Save Format (.sal)

The `.sal` extension does NOT appear in the binary. The Saleae Logic 2 save format is
actually a directory-based format, not a single file with `.sal` extension. The electron
UI handles the `.sal` extension mapping.

The internal serialization goes through WaveformStoreNode (not ExportNode):
- `WaveformStoreNode::OnStartSerialization` -- writes capture data to file
- `WaveformStoreNode::OnStartDeserialization` -- reads capture data from file
- These use a thread-based approach with error handling for:
  - "File is empty"
  - "File not found"
  - "Deserialization exception encountered"

The serialization fields include:
```
/serializationStatus    -> progress tracking
/storedInterval        -> time range of stored data
```

The `.sal` format investigation should be a separate task focused on
WaveformStoreNode serialization, which is distinct from the export system.

## Key Findings

1. **Two formats only**: CSV and Binary. No other export formats in the native code.

2. **CSV output**: `digital.csv` and/or `analog.csv` in the output directory.
   - Digital first, then analog
   - Single file per data type

3. **Binary output**: `digital_N.bin` and `analog_N.bin` per channel.
   - One file per channel
   - Channel index in filename

4. **No .sal in export**: The export system produces CSV or binary files.
   The .sal capture format is handled by WaveformStoreNode serialization, not ExportNode.

5. **ExportWaveformCSV is dead code**: The handler just throws "not implemented".

6. **Cancel support**: Export can be canceled (cancel flag at offset 0xc0).
   On cancel, all created files are deleted.

7. **Progress tracking**: Via callback + state fields reported to UI.

8. **Thread-based**: Export runs on a background thread. Only one export at a time.

9. **Automation API**: Three gRPC RPCs (ExportRawDataCsv, ExportRawDataBinary,
   ExportDataTableCsv) that ultimately invoke the same ExportNode machinery.
