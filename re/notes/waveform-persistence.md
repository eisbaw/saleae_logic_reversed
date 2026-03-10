# Waveform Persistence: WaveformStoreNode, WaveformDataCollection, WaveformSummaryNode

Reverse engineered from `libgraph_server_shared.so` (Saleae Logic 2 v2.4.40).

Source paths leaked via assertions:
- `/workspace/monorepo/graph-io/core/waveform_data/src/waveform_data/waveform_data_collection.cpp`
- `/workspace/monorepo/graph-io/nodes/waveform_store_node/src/waveform_store_node/waveform_store_node.cpp`
- `/workspace/monorepo/graph-io/nodes/waveform_summary_node/src/waveform_summary_node/waveform_summary_node.cpp`
- `/workspace/monorepo/graph-io/nodes/waveform_renderer_node/src/waveform_renderer_node/waveform_render.cpp`
- `/workspace/monorepo/graph-io/nodes/waveform_renderer_node/src/waveform_renderer_node/waveform_renderer_node.cpp`
- `/workspace/monorepo/graph-io/nodes/analog_downsampler_node/src/analog_downsampler_node.cpp`
- `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/mso_device_metadata.cpp`

## Overview

The waveform persistence subsystem handles three responsibilities:

1. **In-memory storage** (WaveformDataCollection) -- holds captured analog waveform
   data in a custom arena allocator during and after capture
2. **Serialization/deserialization** (WaveformStoreNode) -- writes/reads waveform
   data to/from disk using cereal-based binary archives
3. **LOD downsampling** (WaveformSummaryNode + AnalogDownsamplerNode) -- produces
   multi-resolution min/max summaries for efficient waveform rendering at different
   zoom levels

The system operates within Saleae's node graph architecture. Data flows through
pipes between nodes, and serialization/deserialization are triggered by
`StartSerialization` / `StartDeserialization` actions.

## Node Graph Architecture

```
Capture Engine (analog data)
  |
  v  VectorRawWaveformData pipe
WaveformStoreNode
  |  Stores to WaveformDataCollection
  |  Handles serialize/deserialize to disk
  |
  v  WaveformRenderRequest pipe
WaveformSummaryNode
  |  Computes LOD summaries from stored data
  |  Dispatches render requests
  |
  v  WaveformRenderRequest pipe
WaveformRendererNode
  |  Actually renders the waveform for display
  v
RenderingManagerNode
  |  WaveformRenderResult pipe
  v
UI (Electron)
```

Parallel to this, there is an **AnalogDownsamplerNode** that can downsample
analog data before it reaches the store or renderers. The downsample ratio
and mode are configurable at `/downsampleRatio` and `/downsampleMode`.

## Key Classes

| Class | Namespace | Purpose |
|---|---|---|
| WaveformStoreNode | Saleae::Graph | Graph node: receives waveform data, stores in collection, handles ser/deser |
| WaveformDataCollection | Saleae::Graph | In-memory container for analog waveform data (backed by packed_data_collection) |
| WaveformDataCollection::Writer | Saleae::Graph | Appends RawWaveformData into the collection |
| WaveformDataCollection::Reader | Saleae::Graph | Reads back from collection for export/rendering |
| WaveformDataCollection::WaveformIterator | Saleae::Graph | Iterates chunks for serialization |
| WaveformDataCollection::ChunkIterator | Saleae::Graph | Iterates within a chunk for stats computation |
| PackedWaveformData | Saleae::Graph | One chunk of packed analog data (0x90 bytes header + variable data) |
| WaveformSummaryNode | Saleae::Graph | Computes LOD min/max summaries for rendering |
| WaveformRendererNode | Saleae::Graph | Renders waveform data for display |
| AnalogDownsamplerNode | Saleae::Graph | Downsamples analog data before storage |
| WaveformStoreState | Saleae::Graph | Serializable state for the store node |
| AnalogStoreState2 | Saleae::Graph | Serializable state for the analog store |
| SaleaeBinaryOutputArchive | Saleae | Cereal-based binary output archive |
| SaleaeBinaryInputArchive | Saleae | Cereal-based binary input archive |

## WaveformStoreNode

### Node Layout (partial)

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0x150 | 8 | mOutputArchive? | Used for serialization stream binding |
| 0x180 | 40 | mMutex | std::mutex for thread safety |
| 0x1a8 | 8 | mStateCallback | Callback for serialization status updates |
| 0x1b8 | 8 | mSerializationThread | std::thread for async serialize/deserialize |
| 0x1c0 | 1 | mSerializationInProgress | Atomic bool flag |

### Request Handlers

The WaveformStoreNode registers these request handlers during Initialize():

| Action Type | Handler Method |
|---|---|
| NodeActions::StartSerialization | OnStartSerialization |
| NodeActions::StartDeserialization | OnStartDeserialization |
| StoreActions::SetDiscardData | SetDiscardData |
| StoreActions::ExportWaveformCSV | ExportWaveformCSV (NOT IMPLEMENTED - throws) |

### Pipe Handlers

| Data Type | Direction |
|---|---|
| VectorRawWaveformData | Input - received from capture engine |

### Key Functions

| ELF VA | Function | Description |
|--------|----------|-------------|
| 0x2c8b2b0 | CanStartSerialization | Checks if ser/deser is in progress, joins previous thread |
| 0x2c8aa30 | OnStartSerialization | Validates, creates serialization thread |
| 0x2c8a5c0 | OnStartDeserialization | Validates, creates deserialization thread |
| 0x2c8b3c0 | SerializeOnThread | Opens file, writes via cereal archive, reports status |
| 0x2c8c4b0 | DeserializeOnThread | Opens file, reads via cereal archive, updates API |
| 0x2c8f920 | Serialize (inner) | Iterates chunks, writes binary data |
| 0x2c8fd20 | Deserialize (inner) | Reads chunks, inserts into collection |
| 0x2c8fb20 | WriteChunkHeader | Writes PackedWaveformData fields to archive |
| 0x2c8ffb0 | ReadChunkHeader | Reads PackedWaveformData fields from archive |
| 0x2c901f0 | InsertChunk | Inserts deserialized chunk into WaveformDataCollection |
| 0x2c903d0 | FinalizeDeserialization | Updates total count, unlocks mutex |
| 0x2c92050 | UpdateApi | Updates store API after successful deserialization |
| 0x2c8c350 | ReportStatus | Sends serialization status update via callback |
| 0x2c8a570 | ExportWaveformCSV | Stub - throws "not implemented" |

### Serialization State Machine

```
                  CanStartSerialization()
                    |
                    v
  [Idle] ------> [Check mSerializationInProgress]
                    |
         +----------+----------+
         | (flag set)           | (flag clear)
         v                     v
    Return false          Join previous thread (if any)
                               |
                               v
                          Set flag = true
                               |
                               v
                   Create serialization thread
                               |
                               v
              ReportStatus(SerializingOrDeserializing)
                               |
                               v
                    [SerializeOnThread / DeserializeOnThread]
                               |
                               v
                     ReportStatus(Done)
                               |
                               v
                     Clear flag, unlock mutex
```

Status values passed to ReportStatus (at offset 0x1a8):
- 0 = Starting
- 1 = Complete (serialize)
- 2 = Starting (deserialize)
- 3 = Complete (deserialize)
- 4 = Error (file not found)
- 5 = Error (other)

## WaveformDataCollection

### Collection Layout (partial, ~0x500+ bytes)

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x80 | ~0x80 | struct | mIteratorState | Internal iteration state (copied during serialize) |
| 0x100 | ~0x40 | FixedByteDeque | mIndexDeque | Sample index entries (0x10 bytes each) |
| 0x140 | 8 | uint64_t | mIndexCount | Number of index entries |
| 0x200 | ~0x40 | FixedByteDeque | mPointerDeque | Data pointer entries (0x08 bytes each) |
| 0x240 | 8 | uint64_t | mPointerCount | Number of pointer entries |
| 0x280 | 40 | std::mutex | mMutex1 | First lock (for trimming) |
| 0x2A8 | 1 | bool | mDeletionOccurred | Set when data is deleted from end |
| 0x2B0 | ~0x40 | DequeArena | mDataArena | Arena for raw waveform data (0x90 byte chunks) |
| 0x2D8 | 8 | void* | mFreeStart | Current bump pointer in data arena |
| 0x2E0 | 8 | void* | mFreeEnd | End of current slab |
| 0x300 | 40 | std::mutex | mMutex2 | Second lock (for deserialization) |
| 0x328 | ~0x40 | hashtable | mChunkMap | Hash table for chunk lookup |
| 0x330 | 8 | uint64_t | mChunkMapBuckets | Number of hash buckets |
| 0x380 | 8 | uint64_t | mTotalChunkCount | Total number of chunks after deserialization |
| 0x480 | ~0x40 | DequeArena | mStatsArena | Arena for min/max statistics (0x7C bytes per block) |
| 0x500 | ~0x40 | DequeArena | mSerializedArena | Arena for serialized waveform data |

### PackedWaveformData Layout (0x90 bytes header)

Each chunk of waveform data has this header, written/read by WriteChunkHeader/ReadChunkHeader:

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0x00 | 8 | field_0 | (written to archive) |
| 0x08 | 8 | field_8 | (written to archive) |
| 0x10 | 8 | field_10 | (written to archive) |
| 0x18 | 8 | mSampleStart | Start sample number |
| 0x20 | 8 | mSampleEnd | End sample number |
| 0x28 | 8 | field_28 | (written to archive) |
| 0x30 | 8 | field_30 | (written to archive) |
| 0x38 | 8 | mAnalogDataCount | Number of analog samples in this chunk |
| 0x40 | 8 | mAnalogDataPtr | Pointer to analog sample data (int16_t array) |
| 0x48 | 8 | mStatsCount | Number of stats entries in this chunk |
| 0x50 | 8 | mStatsPtr | Pointer to stats data (int16_t array) |
| 0x58 | 8 | field_58 | (written to archive) |
| 0x60 | 1 | field_60 | (written to archive, 1 byte) |
| 0x68 | 8 | field_68 | (written to archive, nested group) |
| 0x70 | 8 | field_70 | (written to archive, nested group) |
| 0x78 | 8 | field_78 | (written to archive, nested group) |
| 0x80 | 8 | field_80 | (written to archive) |
| 0x88 | 2 | field_88 | (written to archive, 2 bytes, nested group) |
| 0x8a | 2 | field_8a | (written to archive, 2 bytes, nested group) |

The analog sample data is stored as **int16_t** (signed 16-bit) values. This
is the raw ADC value (or post-calibration value scaled to 16 bits). The
conversion to volts happens at the rendering/export layer using the
VoltageScale from the channel configuration.

The stats data is also **int16_t** values, stored as min/max pairs for
summary rendering.

### Writer::PushBack

`WaveformDataCollection::Writer::PushBack(const RawWaveformData&)` (ELF VA from
PRETTY_FUNCTION at 0x4fb621):

1. Allocates 0x90 bytes from mDataArena (bump allocation or extend)
2. Copies RawWaveformData header into the arena
3. Adds 0x10 byte index entry to mIndexDeque (sample interval)
4. Adds 0x08 byte pointer entry to mPointerDeque (pointer to arena allocation)
5. Computes min/max summary statistics at multiple power-of-2 reduction levels
   (16, 32, 64, 128, 256, 512 samples per block) -- 6 levels total
6. Stores statistics (0x7C bytes per block) in mStatsArena

### Collection Trimming

`FUN_01b5cfd0` (ELF VA 0x1b5cfd0) trims old data from the collection:
- Acquires both mutexes (0x300 and 0x280)
- Calls FixedByteDeque::PopFront/PopBack to remove index/pointer entries
- Calls DequeArena::FreeBefore/FreeAfter to free slab memory
- Assertion: "attempted to erase more data from collection than exists"

This is the mechanism for pre-trigger buffer management.

## Serialization Format (Binary Archive)

The serialization uses **cereal** library's binary archive format
(`SaleaeBinaryOutputArchive` / `SaleaeBinaryInputArchive`). These are
derived from `cereal::OutputArchive<SaleaeBinaryOutputArchive, 1>` and
`cereal::InputArchive<SaleaeBinaryInputArchive, 1>`.

### Cereal Binary Archive Format

Cereal's binary archive is a simple sequential binary format:
- No field tags or names (unlike JSON or XML archives)
- Values written in host byte order (little-endian on x86)
- Containers prefixed with a `size_type` (uint64_t by default in cereal)
- Nested archives use cereal's traversal order for field ordering

### Waveform Serialization Wire Format

The serialize function (ELF VA 0x2c8f920) writes:

```
[8 bytes]  chunk_count (int64_t) -- number of PackedWaveformData chunks

For each chunk:
  [16 bytes] sample_interval (SampleInterval: start + end, 2x int64_t)

  -- PackedWaveformData header fields (via cereal archive) --
  [8 bytes]  field_0x00
  [8 bytes]  field_0x08
  [8 bytes]  field_0x10
  [8+8 bytes] nested group 1: field_0x18, field_0x20
  [8 bytes]  field_0x28
  [8 bytes]  field_0x30
  [8 bytes]  field_0x58
  [1 byte]   field_0x60
  [8+8+8 bytes] nested group 2: field_0x68, field_0x70, field_0x78
  [8 bytes]  field_0x80
  [2+2 bytes] nested group 3: field_0x88, field_0x8a

  -- Analog sample data --
  [8 bytes]  analog_data_count (int64_t, from field 0x38)
  [N*2 bytes] analog_data (int16_t array, N = analog_data_count)

  -- Stats data --
  [8 bytes]  stats_count (int64_t, from field 0x48)
  [M*2 bytes] stats_data (int16_t array, M = stats_count)
```

The deserialization (ELF VA 0x2c8fd20) reads in the same order, reconstructing
each chunk and inserting it into a new WaveformDataCollection.

### Thread and File I/O

SerializeOnThread:
1. Opens file for writing (std::ofstream via std::filebuf)
2. Wraps in SaleaeBinaryOutputArchive
3. Creates WaveformIterator to iterate all chunks
4. Writes chunk_count, then iterates writing each chunk
5. Logs "serialization complete. total size = N"
6. Reports status = 1 (complete)
7. Clears mSerializationInProgress flag

DeserializeOnThread:
1. Opens file for reading (std::ifstream via std::filebuf)
2. Checks file is not empty (seekg to end, compare positions)
3. Wraps in SaleaeBinaryInputArchive
4. Locks mutex at +0x300
5. Reads chunk_count, then iterates reading each chunk
6. Each chunk: reads header, allocates 0x90 bytes in arena, memcpy, inserts
7. Updates mTotalChunkCount at +0x380
8. Unlocks mutex, calls FUN_01c5cfd0 (collection maintenance)
9. Names thread "WaveformDeserializer"
10. Updates API (FUN_02d92050)
11. Reports status = 3 (complete)

## MSO Metadata Format (ByteBuffer protocol)

The MSO (Mixed Signal Oscilloscope) device uses a separate metadata format
with magic tokens and CRC32. This is NOT the same as the cereal archive
used for waveform data, but both live in the same serialization infrastructure.

### Magic Tokens

Each metadata section begins with a 16-byte ASCII magic token:

| Token | Purpose |
|-------|---------|
| `<SALEAE_MSOMAIN>` | Main MSO metadata block |
| `<SALEAE_MSOLASC>` | Logic Analyzer / Scope Capture metadata |
| `<SALEAE_MSOAFE_>` | Analog Front End calibration data |

`SaleaeMagicTokenLength = 16` (confirmed by assertion at 0x4fb77b).

### CRC Framing

Each metadata section is framed with CRC32:

```
SerializeMagicToken(buffer, token):
  Write 16 bytes: magic token string

StartSerializeCrcAndLength(buffer, state):
  Record current buffer position as state.mBeginDataPosition
  Reserve 1 byte for CRC checksum
  Reserve 2 bytes for section length (uint16_t)

  ... section data ...

FinishSerializeCrcAndLength(buffer, state):
  Assert state.has_value()
  Assert end_position > state.mBeginDataPosition
  Compute CRC8 over section data
  Write CRC8 at state.mBeginDataPosition
  Write uint16_t length at state.mBeginDataPosition + 1
  Seek back to end
```

Source: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/mso_device_metadata.cpp`

Key functions:
| ELF VA | Function |
|--------|----------|
| 0x2d2c6f0 | StartSerializeCrcAndLength |
| 0x2d2c890 | FinishSerializeCrcAndLength |
| 0x2d2cb80 | SerializeMainMetadata (writes SALEAE_MSOMAIN) |
| 0x2d2d010 | SerializeMagicToken (read or write 16-byte token) |
| 0x2d2d390 | Unknown (called after main metadata write) |

CRC function (FUN_02e90d50) computes an 8-bit checksum over the section data.

## WaveformSummaryNode and LOD Downsampling

### Architecture

```
WaveformStoreNode
  |
  +-> WaveformSummaryNode (receives WaveformRenderRequest)
  |     |
  |     +-> Computes LOD from stored data
  |     +-> Forwards render request with pre-computed stats
  |
  +-> WaveformRendererNode (receives WaveformRenderRequest)
        |
        +-> Produces WaveformRenderResult for display
```

The WaveformSummaryNode is set up via `SetApi` (ELF VA 0x2c803b0), which
asserts `waveform_store_api != nullptr`. The store API gives the summary node
read access to the WaveformDataCollection.

### LOD Computation (ChunkIterator::GetStats)

ChunkIterator::GetStats (ELF VA 0x2d03ab0) computes min/max statistics for
a given SampleInterval. It uses two paths:

**Path 1: Raw sample scan (when stats index = 0)**

When the chunk has no precomputed stats (field_0x48 == 0 or for the first
level), it scans the raw int16_t sample data directly:

```cpp
// Unrolled min/max scan over int16_t samples
short* data = (short*)(chunk->mAnalogDataPtr + (start - chunk->mSampleStart) * 2);
short min_val = data[0], max_val = data[0];
for (i = 1; i < count; i += 2) {
    // Compare pairs, tracking running min/max
    if (data[i] > max_val) max_val = data[i];
    if (data[i] < min_val) min_val = data[i];
}
```

**Path 2: Stats-based LOD (when stats index > 0)**

When precomputed stats exist, it uses the stats array which stores min/max
at 256-sample blocks:

```
stats_count / 0x3E (62) = number of 256-sample stats entries per chunk
block_index = sample_offset >> 8  (divide by 256)
```

The function at ELF VA 0x2d037c0 reads individual stats entries, computing:
- Start block = (sample_start - chunk_start) / (stats_block_size * 8)
- End block = similar computation
- For each block, reads min/max from stats array
- Merges into cumulative min/max result

The AnalogStats struct (returned by GetStats) contains at minimum a min/max
pair stored as int16_t values, computed by FUN_02ed1b60 and merged by
FUN_02ed1b70.

### Downsample Modes

The AnalogDownsamplerNode supports at least these modes (from string analysis):

| Field Path | Values |
|---|---|
| `/downsampleMode` | At least: MinMax, possibly Average |
| `/downsampleRatio` | Integer ratio (power of 2 preferred) |
| `/analogLimit_MaxDownsampleRatio` | Maximum allowed ratio |

Assertion: `new_sampling_config.mDownsampleMode != DownsampleMode::MinMax`
-- this is checked to prevent double-application of MinMax downsampling.

Assertion: `new_sampling_config.mDownsampleRatio == Detail::DownsampleRatioFloor(...)`
-- the ratio is floored to a valid value.

### Multi-level Summary Statistics (from TASK-0081)

The WaveformDataCollection::Writer::PushBack function computes min/max summaries
at 6 power-of-2 reduction levels:

| Level | Block Size | Purpose |
|-------|-----------|---------|
| 0 | 16 samples | Finest summary level |
| 1 | 32 samples | |
| 2 | 64 samples | |
| 3 | 128 samples | |
| 4 | 256 samples | Primary rendering LOD |
| 5 | 512 samples | Coarsest summary level |

These are stored in the stats DequeArena (offset 0x500 from collection base,
0x7C bytes per stats block).

## Store Types Comparison

The codebase has three parallel store node implementations:

| Store | Data Type | Source File |
|---|---|---|
| WaveformStoreNode | PackedWaveformData (analog) | waveform_store_node.cpp |
| AnalogStore | SampleData<AnalogData> | (separate, similar pattern) |
| DigitalStore | SampleData<DigitalData> | (separate, similar pattern) |

All three share the same serialization state machine pattern:
- CanStartSerialization (join previous thread, check flag)
- OnStartSerialization (validate, create thread)
- SerializeOnThread (open file, write data, report status)
- OnStartDeserialization (validate, create thread)
- DeserializeOnThread (open file, read data, report status)

The DigitalStore additionally logs "Size of serialized data: {}" before
writing, and "DigitalStore::OnStartSerialization: {} opened" after file open.

## Relationship to .sal File Format

The .sal file is a ZIP archive (or directory) containing per-channel data files.
The serialization system writes individual channel data files, with each
WaveformStoreNode/AnalogStore/DigitalStore responsible for its own file.

The `StartSerialization` action includes a file path (from `/paths` or
`/filePath` in the action JSON). The store node opens this path directly
and writes its cereal binary archive to it.

The `StartDeserialization` action similarly includes a file path. The store
node opens the file, reads the cereal archive, and reconstructs the
WaveformDataCollection.

Graph state coordination (which nodes get serialized, file paths for each)
is handled at a higher level by the Graph framework, not by individual
store nodes.

## Confidence Levels

- **High**: Serialization wire format (directly visible in decompiled serialize/
  deserialize functions with field sizes and order)
- **High**: PackedWaveformData layout offsets (from WriteChunkHeader/ReadChunkHeader)
- **High**: Magic tokens and CRC framing (from explicit string comparisons and
  assertions)
- **High**: WaveformStoreNode state machine (from log messages and control flow)
- **Medium**: Exact field names in PackedWaveformData (inferred from usage patterns,
  not debug symbols)
- **Medium**: LOD block sizes and levels (from arithmetic in GetStats, but the
  exact mapping of levels to sizes could differ)
- **Low**: .sal file directory structure (no direct evidence of ZIP format or
  specific directory layout in the native code -- this may be managed by
  the Electron/JS layer)
- **Low**: ExportWaveformCSV (confirmed as "not implemented" stub)
