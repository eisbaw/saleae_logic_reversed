# Measurement Engine Architecture

Reverse engineering notes for the Saleae Logic 2 measurement subsystem,
covering `AnalogMeasurer`, `DigitalMeasurer`, `RangeMeasurer`,
`MeasurementNode`, and the core `Measurer` class.

## Architecture Overview

The measurement engine has a dual-layer architecture:

1. **C++ orchestration layer** (compiled into `libgraph_server_shared.so`)
   - Graph kernel nodes: `AnalogMeasurer`, `DigitalMeasurer`, `RangeMeasurer`, `MeasurementNode`
   - Manages data flow, store APIs, measurement lifecycle
   - Source paths leaked via assertions (see below)

2. **Python extension layer** (in `pythonlibs/` and extension ZIPs)
   - Two distinct API generations (V1/V2)
   - Measurement logic implemented in Python, called from C++ via embedded Python
   - Extensions loaded dynamically from user-installable packages

### Source Paths (from assertions)

```
/workspace/monorepo/graph-io/nodes/analog_measurer/src/analog_measurer.cpp
/workspace/monorepo/graph-io/nodes/digital_measurer/src/digital_measurer.cpp
/workspace/monorepo/graph-io/nodes/range_measurer/src/range_measurer.cpp
/workspace/monorepo/graph-io/nodes/measurement_node/src/measurement_node/measurement_node.cpp
/workspace/monorepo/graph-io/core/measurements/src/measurements/measurer.cpp
/workspace/monorepo/build/graph-io/linux-x64/release/core/saleae_python/site-packages/saleae/measurements/runner.pyx.cpp
```

## Two Measurement API Generations

### V1 API: `saleae.range_measurements` (streaming/chunked)

The V1 API uses a streaming model where data is delivered in chunks:

```
saleae.range_measurements.Measurer          (base class)
saleae.range_measurements.AnalogMeasurer    (analog extension)
saleae.range_measurements.DigitalMeasurer   (digital extension)
```

**Lifecycle:**
1. Extension declares `supported_measurements` class property (list of metric IDs)
2. `__init__(self, requested_measurements)` -- instantiated with list of enabled metrics
3. `process_data(data)` -- called one or more times with batches of data
4. `measure()` -- called once after all data processed; returns `Dict[str, float]`

**AnalogMeasurer V1:**
- `process_data(data: AnalogData)` receives `AnalogData` objects:
  - `data.samples` -- numpy float32 array of voltages
  - `data.sample_count` -- number of samples
  - `data.raw_samples` -- pre-transform integer samples
  - Voltage transform: `gain * raw + offset`
  - `data.start_time`, `data.end_time` -- `GraphTime` boundaries
- Default `process_data` iterates and calls `process_sample(sample, index)` per sample

**DigitalMeasurer V1:**
- `process_data(data: DigitalData)` receives `DigitalData` objects:
  - Iterating yields `(Time, bitstate)` pairs
  - `bitstate` is `True` for high, `False` for low
  - `Time` supports subtraction to produce float seconds

### V2 API: `saleae.measurements` (span-based, richer output)

The V2 API passes the entire range at once and supports annotations:

```
saleae.measurements.AnalogMeasurer          (base class)
saleae.measurements.Measure                 (measure declaration)
saleae.measurements.MeasureInstance          (runtime measure)
saleae.measurements.MeasureError             (error value)
saleae.measurements.Annotation              (annotation declaration)
saleae.measurements.annotation.HorizontalRule
saleae.measurements.annotation.VerticalRule
```

**Lifecycle:**
1. Measures/annotations declared as class attributes:
   ```python
   class MyMeasurer(AnalogMeasurer):
       min_val = Measure("Min", type=float, units="V")
       max_val = Measure("Max", type=float, units="V")
       rule = Annotation(measures=[min_val, max_val])
   ```
2. `measure_range(self, data: AnalogSpan)` -- called with full span
3. Set `self.min_val.value = ...` and `self.rule.value = HorizontalRule(value=...)`
4. Framework collects results via introspection

**Key differences from V1:**
- Single `measure_range` call instead of streaming `process_data`/`measure`
- Declarative measure/annotation system (metaclass-based)
- `AnalogSpan` richer than `AnalogData` -- provides `min`, `max`, `histogram()`,
  `find_lt()`, `find_ge()`, `rfind_lt()`, `rfind_ge()`, `sample_index_to_time()`
- Measures have type annotations (`float`, `int`, `SaleaeTimeDelta`)
- Values can only be set once (enforced by `MeasureInstance`)
- Annotations link to specific measures and auto-hide when measures are disabled
- `MeasureError` can replace a value when computation is impossible

**API version check:** The V2 API requires `apiVersion >= 1.1.0` in extension.json.
The string `"API version {} is required for saleae.measurements.analog_measurer.AnalogMeasurer"` confirms this.

## Extension Discovery and Loading

### Extension JSON Format

```json
{
  "name": "Extension Name",
  "apiVersion": "1.1.0",
  "version": "0.0.1",
  "extensions": {
    "Display Name": {
      "type": "AnalogMeasurement" | "DigitalMeasurement",
      "entryPoint": "module.ClassName",
      "metrics": {
        "metricId": {
          "name": "Human-readable Name",
          "notation": "HTML notation (e.g. V<sub>RMS</sub>)",
          "units": "V"
        }
      }
    }
  }
}
```

### Bundled Extensions (offline, in `resources/offlineExtensions/`)

1. **saleae-measurements-v2** (V2 API, apiVersion 1.1.0)
   - `VoltageMeasurer`: Peak-to-Peak, Min, Max, Mean, Mid, Amplitude, Top, Base, RMS
   - `PulseMeasurer`: Rise Time, Fall Time, +Overshoot, -Overshoot
   - `TimeMeasurer`: Period, Frequency, Pos Width, Neg Width, Pos Duty, Neg Duty, Cycle RMS

2. **clock-statistics-measurement** (V1 API, apiVersion 1.0.0)
   - `ClockStatsMeasurer` (DigitalMeasurer): Rising Edges, Falling Edges, Avg Frequency,
     Min Frequency, Max Frequency, Period Std Dev

3. **voltage-statistics-measurement** (V1 API, apiVersion 1.0.0)
   - `VoltageStatisticsMeasurer` (AnalogMeasurer): Voltage RMS

## Available Measurement Types

### Analog Measurements (V2 -- VoltageMeasurer)

| Metric | Label | Algorithm | Units |
|--------|-------|-----------|-------|
| minimum | Min | `data.min` (accelerated by AnalogSpan) | V |
| maximum | Max | `data.max` | V |
| peak_to_peak | Peak-to-Peak | `max - min` | V |
| mid | Mid | `(max + min) / 2` | V |
| mean | Mean | `sum(data) / len(data)` | V |
| top | Top | Histogram mode of samples >= mid | V |
| base | Base | Histogram mode of samples < mid | V |
| amplitude | Amplitude | `top - base` | V |
| rms | RMS | `sqrt(sum(x^2) / N)` | V |

### Analog Measurements (V2 -- PulseMeasurer)

| Metric | Label | Algorithm | Units |
|--------|-------|-----------|-------|
| rise_time | Rise Time | Time from 10% to 90% threshold crossing | s |
| fall_time | Fall Time | Time from 90% to 10% threshold crossing | s |
| positive_overshoot | +Overshoot | `max - high` | V |
| negative_overshoot | -Overshoot | `low - min` | V |

Rise/fall time algorithm:
1. Compute high (histogram mode above mid) and low (mode below mid)
2. Thresholds: low_threshold = 10% of (high - low) + low, high = 90%
3. Find first sample below start threshold, then first crossing of start, then crossing of end
4. Rise time = time delta between crossings

### Analog Measurements (V2 -- TimeMeasurer)

| Metric | Label | Algorithm | Units |
|--------|-------|-----------|-------|
| period | Period | Time between same-direction crossings of mid | s |
| frequency | Frequency | `1 / period` | Hz |
| positive_width | Pos Width | Time spent above mid in one cycle | s |
| negative_width | Neg Width | Time spent below mid in one cycle | s |
| positive_duty | Pos Duty | `pos_width / period * 100` | % |
| negative_duty | Neg Duty | `neg_width / period * 100` | % |
| cycle_rms | Cycle RMS | RMS over exactly one cycle | V |

Cycle detection algorithm:
1. Start at center of span
2. Find crossing of mid-level to left and right
3. Find a third crossing to determine full cycle
4. Use cycle boundaries for period/duty computation

### Digital Measurements (V1 -- ClockStatsMeasurer)

| Metric | Label | Algorithm | Units |
|--------|-------|-----------|-------|
| edgesRising | No. Rising Edges | Count of low-to-high transitions | count |
| edgesFalling | No. Falling Edges | Count of high-to-low transitions | count |
| frequencyAvg | Average Frequency | `period_count / total_time` | Hz |
| frequencyMin | Minimum Frequency | `1 / max_period` | Hz |
| frequencyMax | Maximum Frequency | `1 / min_period` | Hz |
| periodStdDev | Period STD | Welford's online algorithm for variance, then sqrt | s |

### Analog Measurements (V1 -- VoltageStatisticsMeasurer)

| Metric | Label | Algorithm | Units |
|--------|-------|-----------|-------|
| voltageRms | Voltage RMS | Running online `numpy.average(numpy.square(data.samples))` | V |

## C++ Orchestration Layer

### Graph Kernel Nodes

The measurement system integrates into the Saleae graph processing engine.
Each measurer is a kernel node registered via `KernelFactory::AddKernel<T>`.

**Node hierarchy:**

```
MeasurementNode (coordinator)
  |-- handles MeasureRangeRequest, MeasureRangeAllRequest,
  |   MeasureInstantenousRequest, ScanMeasurersRequest,
  |   SetMeasurementConfigurationRequest
  |
  +-- AnalogMeasurer (kernel node)
  |     Sets up analog store API
  |     Dispatches to Python measurer via MeasurerInstance
  |
  +-- DigitalMeasurer (kernel node)
  |     Sets up digital store API
  |     Dispatches to Python measurer via MeasurerInstance
  |
  +-- RangeMeasurer (kernel node)
        Handles range-based measurements
        Dispatches to either analog or digital store
```

### Key C++ Classes (from mangled symbols)

```
Saleae::Graph::MeasurementNode
Saleae::Graph::MeasurementNodeApi
Saleae::Graph::AnalogMeasurer
Saleae::Graph::DigitalMeasurer
Saleae::Graph::RangeMeasurer
Saleae::Graph::MeasurerInstance
Saleae::Measurer                    (core, wraps Python)
Saleae::MeasurerPath
Saleae::MeasurementResult
Saleae::Python::WaveformWrapper
```

### Request/Response Types

```
MeasurementRequestData          -- request to run a measurement
MeasurementData                 -- measurement result
MeasurementsResultData          -- aggregated results
MeasurementConfiguration        -- measurement config (from SetMeasurementConfigurationRequest)
MeasureRangeRequest             -- range measurement request
MeasureRangeAllRequest          -- measure all in range
MeasureInstantenousRequest      -- point measurement (not currently used, commented out in Python)
RangeMeasurementRequestData     -- range-specific request data
RangeMeasurementData            -- range-specific result data
ScanMeasurersRequest             -- discover available measurers
ScanMeasurersResult              -- list of available measurers
SetMeasurementConfigurationRequest -- configure measurements
RangeMeasurerPythonSpecData     -- Python measurer specification
```

### Pipe Channels

Communication between nodes uses named pipe channels:
- `/measurer` -- measurer identification
- `/measurers` -- list of measurers
- `/measures` -- measure definitions
- `/measureNames` -- human-readable names
- `/measuredValues` -- computed values
- `/measurementResults` -- results channel
- `/measurerResults` -- per-measurer results
- `/measureResults` -- aggregated results
- `/dependentMeasures` -- dependency graph
- `/annotationResults` -- annotation output
- `/annotations` -- annotation definitions
- `/supportsPointMeasurement` -- capability flag
- `/supportsRangeMeasurement` -- capability flag
- `/frequency` -- frequency data channel
- `/period` -- period data channel
- `/initialFrequency` -- initial frequency estimate
- `/finalFrequency` -- final frequency result

### Key Function Addresses (ELF VAs)

| Function | ELF VA | Size | Description |
|----------|--------|------|-------------|
| AnalogMeasurer::ctor | 0x1acbab0 | 49B | Constructor, sets vtable |
| AnalogMeasurer::Initialize (lambda) | 0x1acc2a0 | 489B | Sets up analog store API, dispatches |
| DigitalMeasurer::Initialize (lambda) | 0x1adf160 | 489B | Sets up digital store API, dispatches |
| RangeMeasurer::HandleMeasurementRequest | 0x1b44700 | 859B | Orchestrates range measurement |
| RangeMeasurer::Initialize | 0x1b440c0 | 218B | Validates store API presence |
| MeasurementNode::MeasureRange | 0x1bfd130 | 5864B | Main measurement orchestrator |
| MeasurementNode::ClearApi | 0x1bfec80 | 117B | Cleanup |
| MeasurementNode name ctor | 0x1c02490 | 12B | Registers "Saleae::Graph::MeasurementNode" |
| Measurer::ctor | 0x21055f0 | 1290B | Loads Python module, initializes |
| Measurer::RunMeasurements (analog) | 0x210a4c0 | 2295B | Runs measurements on analog data |
| Measurer::RunMeasurements (digital) | 0x21097e0 | 2295B | Runs measurements on digital data |
| Measurer validate/init | 0x2105e00 | 308B | Validates measurer has required methods |
| Measurer load extension | 0x21065e0 | 8002B | Loads Python extension, validates API |
| Measurer process measures (analog) | 0x210bca0 | 7456B | Processes measure values from Python |
| Measurer process measures (digital) | 0x210fd10 | 7542B | Processes measure values from Python |
| Runner init | 0x218a1b0 | 2741B | Initializes saleae.measurements.runner |
| Runner run_measurement | 0x2187ec0 | 5624B | Calls run_measurement in Python |
| Frequency estimator | 0x2d5c600 | 2074B | Estimated frequency calculation |
| RangeMeasurerPythonSpecData ctor | 0x2cb81f0 | 54B | Python spec data |

## Execution Flow

### Point Measurement (V2 API)

```
1. MeasurementNode receives MeasureInstantenousRequest
   (currently disabled -- measure_point is commented out in Python)

2. Not implemented in current version
```

### Range Measurement (V2 API)

```
1. MeasurementNode receives MeasureRangeRequest or MeasureRangeAllRequest
2. MeasurementNode::MeasureRange called with MeasurementConfiguration + GraphInterval
3. For each active measurer:
   a. MeasurerInstance created with Python module reference
   b. If analog: RunMeasurementsAnalog called
      - Creates WaveformWrapper or reads from AnalogDataCollection
      - Calls Python measure_range(AnalogSpan)
      - Collects results from MeasureInstance values
   c. If digital: RunMeasurementsDigital called
      - Reads from DigitalDataCollection
      - Calls Python measure_range(data) or measure_point(data, point)
      - Collects results from MeasureInstance values
4. Results aggregated into MeasurementsResultData
5. Annotations extracted from AnnotationInstance values
6. Results sent via /measurementResults pipe
```

### Range Measurement (V1 API)

```
1. RangeMeasurer receives RangeMeasurementRequestData
2. RangeMeasurer::HandleMeasurementRequest dispatched to task thread
3. Store API queried (analog or digital) to get data for range
4. MeasurerInstance created from RangeMeasurerPythonSpecData
5. If analog:
   a. Data batched from AnalogDataCollection::Reader
   b. process_data(AnalogData) called for each batch
   c. measure() called to collect results
6. If digital:
   a. Data read from DigitalDataCollection::Reader
   b. process_data(DigitalData) called for each batch
   c. measure() called to collect results
7. Results sent as RangeMeasurementData
```

## Data Types

### SaleaeTimeDelta (Cython, compiled into libgraph_server_shared.so)

```python
SaleaeTimeDelta(second=0, millisecond=0, *, microsecond=0, nanosecond=0, picosecond=0)
```

Supports: `__add__`, `__sub__`, `__mul__`, `__truediv__`, `__float__`, comparisons.
Used for time-based measurement results.

### GraphInterval / SampleInterval

Internal C++ types representing spans of time or samples.
- `GraphInterval` -- a time range in the graph
- `SampleInterval` -- a range of sample indices

### VoltageScale

Analog voltage scaling parameters passed to measurement functions.

### AnalogSpan (Cython, for V2 API)

Rich analog data accessor with:
- `data.min`, `data.max` -- accelerated min/max
- `data.histogram()` -- histogram computation
- `data.find_lt(threshold)`, `data.find_ge(threshold)` -- threshold crossing search
- `data.rfind_lt()`, `data.rfind_ge()` -- reverse search
- `data.sample_index_to_time(index)` -- index to timestamp conversion
- `data[start:end]` -- slicing support
- `len(data)` -- sample count
- Iteration yields voltage values

### AnalogData (for V1 API)

Simpler voltage data container:
- `data.raw_samples` -- numpy ndarray of integer ADC values
- `data.samples` -- numpy float32 array after voltage transform
- `data.sample_count` -- number of samples
- Voltage transform: `gain * raw_sample + offset`

### DigitalData (for V1 API)

Digital transition data:
- Iteration yields `(Time, bitstate)` pairs
- `bitstate`: `True` = high, `False` = low
- `Time` supports subtraction to produce float seconds

## Key Observations

1. **Measurements are post-capture, not on-the-fly.** They operate on stored waveform
   data (AnalogDataCollection, DigitalDataCollection) via store APIs. The "range" in
   RangeMeasurer refers to a time range selected by the user, not a streaming window.

2. **Python is the extension mechanism.** All actual measurement algorithms run in
   embedded Python 3.8. The C++ layer handles data access, graph integration, and
   result serialization.

3. **Two API versions coexist.** V1 (streaming batches) and V2 (full-span with
   declarative measures/annotations). V2 is preferred for new extensions.

4. **Point measurements are not implemented.** The `measure_point` method is
   commented out in the V2 API source and flagged as "not currently supported."

5. **Frequency estimation has a separate C++ path.** The function at ELF VA 0x2d5c600
   (2074 bytes) implements frequency estimation with the string "Estimated frequency: {}",
   suggesting some measurement computation happens in C++ outside the Python layer.

6. **Error handling is explicit.** Both APIs support setting `MeasureError` when
   computation fails (e.g., "Edge Not Found", "3rd Crossing Not Found").

7. **High/Low computation uses histogram mode.** Rather than simple min/max, the
   "Top" and "Base" measurements use histogram mode to find the most common high/low
   voltage levels, which is more robust for noisy signals.

8. **Welford's algorithm for online variance.** The clock statistics measurer uses
   Welford's online algorithm for computing period standard deviation in a single pass,
   avoiding numerical instability.

9. **Thread dispatch.** RangeMeasurer dispatches measurement to a task thread
   (via `Saleae::Tasks::TaskHandle`), keeping the graph processing thread responsive.

10. **Scan/discovery mechanism.** `ScanMeasurersRequest`/`ScanMeasurersResult` allow
    the UI to discover available measurement extensions dynamically.
