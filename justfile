# Saleae Logic 2 Reverse Engineering - Project Commands
# Run all recipes inside nix-shell: nix-shell --run 'just <recipe>'

set dotenv-load := false

# Saleae binary locations (from shell.nix $SALEAE_PATH)
saleae_path     := env("SALEAE_PATH", "")
main_so         := saleae_path / "resources/linux-x64/libgraph_server_shared.so"
asar_file       := saleae_path / "resources/app.asar"

# Saleae hardware is attached locally (was nvdk-msdlab-09, now local)

# Saleae vendor IDs for USB filtering
saleae_vid      := "21a9"
saleae_vid_legacy := "0925"

# ─── Default ────────────────────────────────────────────────────────────────

# List available recipes
default:
    @just --list --unsorted

# ─── Environment ────────────────────────────────────────────────────────────

# Verify that required tools are available and USB access is configured
check:
    #!/usr/bin/env bash
    set -euo pipefail
    ok=0; fail=0
    check() {
        if command -v "$1" &>/dev/null; then
            printf "  [ok]   %-20s %s\n" "$1" "$(command -v "$1")"
            ok=$((ok + 1))
        else
            printf "  [MISS] %-20s not found\n" "$1"
            fail=$((fail + 1))
        fi
    }
    echo "=== Tool Check ==="
    # RE tools
    check ghidra-analyzeHeadless
    check binwalk
    check tshark
    check frida
    check ltrace
    check strace
    check asar
    check nm
    check readelf
    check strings
    check xxd
    check python3
    # Rust tools
    check cargo
    check rustc
    check clippy-driver
    check rustfmt
    check cargo-nextest
    # Project tooling
    check just
    check pkg-config

    echo ""
    echo "=== Saleae Path ==="
    if [ -n "{{ saleae_path }}" ] && [ -d "{{ saleae_path }}" ]; then
        echo "  [ok]   SALEAE_PATH={{ saleae_path }}"
    elif [ -n "{{ saleae_path }}" ]; then
        echo "  [MISS] SALEAE_PATH={{ saleae_path }} (directory not found)"
        fail=$((fail + 1))
    else
        echo "  [MISS] SALEAE_PATH not set (are you in nix-shell?)"
        fail=$((fail + 1))
    fi

    if [ -f "{{ main_so }}" ]; then
        echo "  [ok]   libgraph_server_shared.so found"
    else
        echo "  [MISS] libgraph_server_shared.so not found at {{ main_so }}"
        fail=$((fail + 1))
    fi

    echo ""
    echo "=== Local USB ==="
    if lsusb -d {{ saleae_vid }}: 2>/dev/null | grep -q .; then
        dev=$(lsusb -d {{ saleae_vid }}:)
        echo "  [ok]   Saleae device found: $dev"
    else
        echo "  [WARN] No Saleae device attached locally"
    fi

    echo ""
    echo "=== Summary ==="
    echo "  $ok ok, $fail missing/failed"
    [ "$fail" -eq 0 ]

# Install udev rules for Saleae devices (requires sudo)
udev-install:
    #!/usr/bin/env bash
    set -euo pipefail
    RULES_FILE="/etc/udev/rules.d/99-saleae.rules"
    echo "Writing Saleae udev rules to $RULES_FILE"
    sudo tee "$RULES_FILE" > /dev/null << 'UDEV'
    # Saleae Logic analyzers
    SUBSYSTEM=="usb", ATTR{idVendor}=="0925", ATTR{idProduct}=="3881", MODE="0666"
    SUBSYSTEM=="usb", ATTR{idVendor}=="21a9", ATTR{idProduct}=="1001", MODE="0666"
    SUBSYSTEM=="usb", ATTR{idVendor}=="21a9", ATTR{idProduct}=="1003", MODE="0666"
    SUBSYSTEM=="usb", ATTR{idVendor}=="21a9", ATTR{idProduct}=="1004", MODE="0666"
    SUBSYSTEM=="usb", ATTR{idVendor}=="21a9", ATTR{idProduct}=="1005", MODE="0666"
    SUBSYSTEM=="usb", ATTR{idVendor}=="21a9", ATTR{idProduct}=="1006", MODE="0666"
    UDEV
    sudo udevadm control --reload-rules
    sudo udevadm trigger
    echo "udev rules installed and reloaded."

# Print the Saleae nix store path
saleae-path:
    @if [ -n "{{ saleae_path }}" ]; then \
        echo "{{ saleae_path }}"; \
    else \
        echo "ERROR: SALEAE_PATH not set. Are you in nix-shell?" >&2; \
        exit 1; \
    fi

# ─── RE - Decomposition ────────────────────────────────────────────────────

# Extract app.asar to re/extracted-asar/
extract-asar:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{ asar_file }}" ]; then
        echo "ERROR: asar not found at {{ asar_file }}" >&2
        echo "Is SALEAE_PATH set? Are you in nix-shell?" >&2
        exit 1
    fi
    OUT="re/extracted-asar"
    rm -rf "$OUT"
    mkdir -p "$OUT"
    echo "Extracting {{ asar_file }} -> $OUT/"
    asar extract "{{ asar_file }}" "$OUT"
    echo "Done. $(find "$OUT" -type f | wc -l) files extracted."

# Search for PATTERN across all Saleae binaries (strings + grep)
strings-search PATTERN:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{ saleae_path }}" ]; then
        echo "ERROR: SALEAE_PATH not set" >&2; exit 1
    fi
    echo "Searching for '{{ PATTERN }}' in Saleae binaries..."
    find "{{ saleae_path }}" \( -name '*.so' -o -name '*.node' -o -name 'Logic' \) \
        -exec sh -c 'strings "$1" | grep -i "{{ PATTERN }}" && echo "  ^^^ $1"' _ {} \; \
        2>/dev/null || echo "(no matches)"

# Dump symbols from a library (nm + readelf). LIB is relative to SALEAE_PATH/resources/linux-x64/
symbols LIB="libgraph_server_shared.so":
    #!/usr/bin/env bash
    set -euo pipefail
    TARGET="{{ saleae_path }}/resources/linux-x64/{{ LIB }}"
    if [ ! -f "$TARGET" ]; then
        echo "ERROR: $TARGET not found" >&2; exit 1
    fi
    echo "=== nm --demangle --dynamic $TARGET ==="
    nm --demangle --dynamic "$TARGET" 2>/dev/null | head -200
    echo ""
    echo "(truncated to 200 lines; pipe to file for full output)"
    echo ""
    echo "=== readelf --dyn-syms $TARGET | head -100 ==="
    readelf --dyn-syms "$TARGET" | head -100

# Launch Ghidra with the RE project (creates project if needed)
ghidra-open:
    #!/usr/bin/env bash
    set -euo pipefail
    PROJECT_DIR="re/ghidra"
    PROJECT_NAME="saleae_re"
    mkdir -p "$PROJECT_DIR"
    if [ ! -f "$PROJECT_DIR/$PROJECT_NAME.gpr" ]; then
        echo "No Ghidra project found. Creating new project..."
        echo "Import {{ main_so }} after Ghidra opens."
    fi
    ghidra "$PROJECT_DIR" "$PROJECT_NAME" &
    echo "Ghidra launched in background."

# Export decompiled C code from Ghidra for AI analysis
ghidra-export:
    #!/usr/bin/env bash
    set -euo pipefail
    PROJECT_DIR="re/ghidra"
    PROJECT_NAME="saleae_re"
    SCRIPTS_DIR="re/ghidra/scripts"
    OUTPUT_DIR="re/ghidra/decompiled"
    if [ ! -d "$PROJECT_DIR/$PROJECT_NAME.rep" ]; then
        echo "ERROR: No Ghidra project found. Run 'just ghidra-import' first." >&2
        echo "Then run 'just ghidra-fid' to label library functions." >&2
        exit 1
    fi
    # Clean previous export
    rm -rf "$OUTPUT_DIR"
    mkdir -p "$OUTPUT_DIR"
    echo "Exporting decompiled C code to $OUTPUT_DIR/"
    echo "This will take a long time for a 48MB binary (potentially hours)."
    echo ""
    ghidra-analyzeHeadless "$PROJECT_DIR" "$PROJECT_NAME" \
        -process libgraph_server_shared.so \
        -noanalysis \
        -scriptPath "$SCRIPTS_DIR" \
        -postScript ExportDecompiled.py "$OUTPUT_DIR" \
        -log "$PROJECT_DIR/export.log"
    echo ""
    echo "Export complete. See:"
    echo "  $OUTPUT_DIR/index.txt     - function index"
    echo "  $OUTPUT_DIR/xrefs.txt     - cross-reference graph"
    echo "  $OUTPUT_DIR/classes.txt   - detected class groupings"
    echo "  $OUTPUT_DIR/by-address/   - individual decompiled .c files"
    echo "  $OUTPUT_DIR/by-class/     - class-grouped symlinks"

# Import libgraph_server_shared.so into a Ghidra headless project with full analysis
ghidra-import:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{ main_so }}" ]; then
        echo "ERROR: {{ main_so }} not found" >&2; exit 1
    fi
    PROJECT_DIR="re/ghidra"
    PROJECT_NAME="saleae_re"
    mkdir -p "$PROJECT_DIR"
    echo "Importing {{ main_so }} into Ghidra project..."
    echo "This will take a long time (~30-60 min for a 48MB binary)."
    ghidra-analyzeHeadless "$PROJECT_DIR" "$PROJECT_NAME" \
        -import "{{ main_so }}" \
        -overwrite \
        -analysisTimeoutPerFile 3600 \
        -log "$PROJECT_DIR/import.log"
    echo "Import complete. Project: $PROJECT_DIR/$PROJECT_NAME"

# Run FunctionID + string-based labeling on the analyzed binary
ghidra-fid:
    #!/usr/bin/env bash
    set -euo pipefail
    PROJECT_DIR="re/ghidra"
    PROJECT_NAME="saleae_re"
    REPORTS_DIR="re/ghidra/reports"
    SCRIPTS_DIR="re/ghidra/scripts"
    mkdir -p "$REPORTS_DIR"
    if [ ! -d "$PROJECT_DIR/$PROJECT_NAME.rep" ]; then
        echo "ERROR: No Ghidra project found. Run 'just ghidra-import' first." >&2
        exit 1
    fi
    echo "Running FunctionID analysis..."
    ghidra-analyzeHeadless "$PROJECT_DIR" "$PROJECT_NAME" \
        -process libgraph_server_shared.so \
        -noanalysis \
        -scriptPath "$SCRIPTS_DIR" \
        -postScript ApplyFunctionID.py "$REPORTS_DIR" \
        -postScript LabelByStrings.py "$REPORTS_DIR" \
        -log "$PROJECT_DIR/fid.log"
    echo "Reports written to $REPORTS_DIR/"
    echo "See: $REPORTS_DIR/fid-report.txt"
    echo "See: $REPORTS_DIR/string-label-report.txt"

# Generate FunctionID databases from nix store library versions
ghidra-gen-sigs:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Generating FunctionID databases from nix store libraries..."
    bash re/ghidra/scripts/generate-fid-from-nix.sh re/ghidra/fid/

# Run binwalk entropy and signature scan on libgraph_server_shared.so
binwalk-scan:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{ main_so }}" ]; then
        echo "ERROR: {{ main_so }} not found" >&2; exit 1
    fi
    echo "=== Signature Scan ==="
    binwalk "{{ main_so }}"
    echo ""
    echo "=== Entropy Analysis ==="
    binwalk -E "{{ main_so }}"

# Extract embedded firmware and FPGA bitstream blobs to firmware/
extract-blobs:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{ main_so }}" ]; then
        echo "ERROR: {{ main_so }} not found" >&2
        echo "Is SALEAE_PATH set? Are you in nix-shell?" >&2
        exit 1
    fi
    python3 re/scripts/extract-blobs.py "{{ main_so }}"

# Extract firmware blobs (alias for extract-blobs)
extract-firmware: extract-blobs

# Parse GPIF II state machine descriptor from FX3 firmware
parse-gpif FW="firmware/fw-fx3-pro8.img":
    python3 re/scripts/parse-gpif.py "{{ FW }}"

# List all .so files in Saleae distribution with sizes and types
list-libs:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{ saleae_path }}" ]; then
        echo "ERROR: SALEAE_PATH not set" >&2; exit 1
    fi
    echo "=== Shared Libraries in {{ saleae_path }} ==="
    find "{{ saleae_path }}" -name '*.so' -o -name '*.so.*' -o -name '*.node' | sort | while read -r f; do
        sz=$(stat -c%s "$f" 2>/dev/null || echo "?")
        typ=$(file -b "$f" 2>/dev/null | head -c 80)
        printf "  %10s  %-60s  %s\n" "$sz" "${f#{{ saleae_path }}/}" "$typ"
    done

# ─── RE - USB ───────────────────────────────────────────────────────────────

# Capture USB traffic from Saleae device with a named operation label
# Usage: just usb-capture idle                      (5s default)
#        just usb-capture firmware-upload 30         (30s)
#        just usb-capture digital-capture 60 "1ch 1MHz"
usb-capture OPERATION DURATION="5" NOTES="":
    #!/usr/bin/env bash
    set -euo pipefail
    ARGS="-d {{ DURATION }}"
    if [ -n "{{ NOTES }}" ]; then
        ARGS="$ARGS -n '{{ NOTES }}'"
    fi
    eval bash re/scripts/usb-capture.sh "{{ OPERATION }}" $ARGS

# Quick unlabeled USB capture for DURATION seconds (tcpdump, local)
usb-capture-quick DURATION="10":
    #!/usr/bin/env bash
    set -euo pipefail
    TIMESTAMP=$(date +%Y%m%d-%H%M%S)
    OUTFILE="re/captures/quick_${TIMESTAMP}.pcap"
    mkdir -p re/captures
    BUS=$(lsusb -d {{ saleae_vid }}: | head -1 | sed 's/Bus \([0-9]*\).*/\1/' | sed 's/^0*//')
    echo "Capturing USB traffic on local bus $BUS for {{ DURATION }}s..."
    echo "Output: $OUTFILE"
    sudo modprobe usbmon 2>/dev/null || true
    sudo timeout $(({{ DURATION }} + 2)) tcpdump -i usbmon${BUS} -w "$OUTFILE" 2>/dev/null || true
    echo "Capture complete: $(stat -c%s "$OUTFILE" 2>/dev/null || echo '?') bytes"
    echo "Analyze with: tshark -r $OUTFILE"

# List USB devices locally, highlighting Saleae devices
usb-list:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Local USB devices ==="
    lsusb | while read -r line; do
        if echo "$line" | grep -qiE '{{ saleae_vid }}|{{ saleae_vid_legacy }}'; then
            echo ">>> $line  <<< SALEAE"
        else
            echo "    $line"
        fi
    done

# Full USB descriptor dump for Saleae devices
usb-descriptors:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Saleae USB Descriptors (local) ==="
    lsusb -v -d {{ saleae_vid }}: 2>/dev/null || true
    lsusb -v -d {{ saleae_vid_legacy }}: 2>/dev/null || true

# Live USB monitor (tcpdump, raw output, Ctrl-C to stop)
usb-monitor:
    #!/usr/bin/env bash
    set -euo pipefail
    BUS=$(lsusb -d {{ saleae_vid }}: | head -1 | sed 's/Bus \([0-9]*\).*/\1/' | sed 's/^0*//')
    echo "Live USB monitor on local bus $BUS (Ctrl-C to stop)..."
    sudo modprobe usbmon 2>/dev/null || true
    sudo tcpdump -i usbmon${BUS} -nn -v

# Check USB capture prerequisites locally
usb-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== USB Capture Prerequisites (local) ==="
    echo ""
    echo "--- Device ---"
    lsusb -d {{ saleae_vid }}: 2>/dev/null && echo "  [ok]" || echo "  [MISSING] No Saleae device"
    echo ""
    echo "--- tcpdump ---"
    which tcpdump && tcpdump --version 2>&1 | head -1 && echo "  [ok]" || echo "  [MISSING]"
    echo ""
    echo "--- tshark ---"
    which tshark 2>/dev/null && echo "  [ok]" || echo "  [NOT INSTALLED]"
    echo ""
    echo "--- usbmon ---"
    sudo modprobe usbmon 2>/dev/null && ls /dev/usbmon* 2>/dev/null && echo "  [ok]" || echo "  [ISSUE] Cannot load usbmon"
    echo ""
    echo "--- sudo ---"
    sudo -n true 2>/dev/null && echo "  [ok] passwordless sudo" || echo "  [WARN] sudo requires password"

# ─── Rust ───────────────────────────────────────────────────────────────────

_require-cargo:
    @if [ ! -f Cargo.toml ]; then \
        echo "ERROR: No Cargo.toml found. Rust workspace not yet initialized." >&2; \
        echo "Create it with: cargo init --lib crates/saleae-proto && cargo init --lib crates/saleae-usb && ..." >&2; \
        exit 1; \
    fi

# Build all crates
build: _require-cargo
    cargo build

# Run all tests
test: _require-cargo
    cargo nextest run

# Run clippy lints (allow dead_code for staged-but-unwired modules)
clippy: _require-cargo
    cargo clippy --all-targets --all-features -- -D warnings -A dead_code

# Format all code
fmt: _require-cargo
    cargo fmt --all

# Check formatting without modifying files
fmt-check: _require-cargo
    cargo fmt --all -- --check

# Run clippy + format check
lint: clippy fmt-check

# Run end-to-end tests (requires hardware or mock)
e2e: _require-cargo
    @echo "NOTE: e2e tests not yet implemented."
    @echo "Will run integration tests requiring USB device access."
    cargo nextest run -E 'test(e2e)' || echo "(no e2e tests found yet)"

# Run the CLI binary
run *ARGS: _require-cargo
    cargo run --bin saleae-cli -- {{ ARGS }}

# Generate and open documentation
doc: _require-cargo
    cargo doc --no-deps --open

# ─── Hardware ──────────────────────────────────────────────────────────────

# Default paths for firmware and bitstream blobs
fw_path     := "firmware/fw-fx3-pro8.img"
bs_path     := "firmware/bitstream-spartan6-slx9-03.bin"

# Build release binary (shared dependency for hw recipes)
_build-release: _require-cargo
    cargo build --release --bin saleae-cli

# Build release binary and run against local hardware
run-hw *ARGS: _build-release
    target/release/saleae-cli {{ ARGS }}

# Ping the device (verify firmware is running and EP1 is alive)
hw-ping: _build-release
    target/release/saleae-cli -v ping

# Full pipeline: firmware upload, FPGA program, ADC init, 2s capture, analog decode, teardown
hw-capture: _build-release
    target/release/saleae-cli -vv full-init \
        --firmware {{ fw_path }} --bitstream {{ bs_path }}

# Digital capture (default: all 8 channels, 1MHz, 2s, CSV output)
hw-capture-digital RATE="1000000" DURATION="2.0" CHANNELS="0,1,2,3,4,5,6,7" OUTPUT="capture-out": _build-release
    target/release/saleae-cli -v capture-digital \
        --firmware {{ fw_path }} --bitstream {{ bs_path }} \
        --sample-rate {{ RATE }} --duration {{ DURATION }} \
        --channels {{ CHANNELS }} --output {{ OUTPUT }} --format csv

# Analog capture (default: channels 0,1, 1MHz, 2s, CSV output)
hw-capture-analog RATE="1000000" DURATION="2.0" CHANNELS="0,1" OUTPUT="capture-out": _build-release
    target/release/saleae-cli -v capture-analog \
        --firmware {{ fw_path }} --bitstream {{ bs_path }} \
        --sample-rate {{ RATE }} --duration {{ DURATION }} \
        --channels {{ CHANNELS }} --output {{ OUTPUT }} --format csv

# Quick capture (minimal init, digital-only, raw EP2 dump)
hw-capture-quick DURATION="2.0" OUTPUT="": _build-release
    #!/usr/bin/env bash
    set -euo pipefail
    ARGS="-v capture-quick --firmware {{ fw_path }} --bitstream {{ bs_path }} --duration {{ DURATION }}"
    if [ -n "{{ OUTPUT }}" ]; then
        ARGS="$ARGS --output {{ OUTPUT }}"
    fi
    target/release/saleae-cli $ARGS

# Reset device to bootloader mode (requires power cycle after)
hw-reset: _build-release
    target/release/saleae-cli reset-device

# Upload firmware only (no FPGA/ADC init)
hw-upload-fw: _build-release
    target/release/saleae-cli upload-firmware --firmware {{ fw_path }}

# Program FPGA only (assumes firmware already running)
hw-program-fpga: _build-release
    target/release/saleae-cli -v program-fpga --bitstream {{ bs_path }}

# Full hardware smoke test: init + short digital capture + short analog capture
hw-smoke: _build-release
    #!/usr/bin/env bash
    set -euo pipefail
    CLI="target/release/saleae-cli"
    echo "=== Full Init ==="
    $CLI -vv full-init --firmware {{ fw_path }} --bitstream {{ bs_path }}
    echo ""
    echo "=== Digital Capture (1s, 1MHz, all channels) ==="
    $CLI -v capture-digital --firmware {{ fw_path }} --bitstream {{ bs_path }} \
        --sample-rate 1000000 --duration 1.0 --channels 0,1,2,3,4,5,6,7 \
        --output /tmp/saleae-smoke-digital --format csv
    echo ""
    echo "=== Analog Capture (1s, 1MHz, ch 0,1) ==="
    $CLI -v capture-analog --firmware {{ fw_path }} --bitstream {{ bs_path }} \
        --sample-rate 1000000 --duration 1.0 --channels 0,1 \
        --output /tmp/saleae-smoke-analog --format csv
    echo ""
    echo "=== Smoke Test Complete ==="
    echo "Digital output: /tmp/saleae-smoke-digital/"
    echo "Analog output:  /tmp/saleae-smoke-analog/"

# Comprehensive capture matrix: digital, analog, mixed at various rates and channel configs
hw-capture-matrix: _build-release
    #!/usr/bin/env bash
    set -euo pipefail
    CLI="target/release/saleae-cli"
    FW="{{ fw_path }}"
    BS="{{ bs_path }}"
    OUT="/tmp/saleae-capture-matrix"
    rm -rf "$OUT"
    mkdir -p "$OUT"
    PASS=0; FAIL=0; SKIP=0
    run_test() {
        local name="$1"; shift
        echo "--- $name ---"
        local dir="$OUT/$name"
        mkdir -p "$dir"
        local log="$OUT/$name.log"
        if "$CLI" -v "$@" --output "$dir" --format csv >"$log" 2>&1; then
            tail -5 "$log"
            echo "  PASS"
            PASS=$((PASS + 1))
        else
            local rc=$?
            tail -10 "$log"
            echo "  FAIL (exit $rc)"
            FAIL=$((FAIL + 1))
        fi
        echo ""
    }

    echo "=== Capture Matrix Test ==="
    echo "Output: $OUT"
    echo ""

    # --- Digital captures ---
    echo "==== Digital Captures ===="
    run_test "digital-1ch-1mhz" \
        capture-digital --firmware "$FW" --bitstream "$BS" \
        --sample-rate 1000000 --duration 1.0 --channels 0

    run_test "digital-4ch-1mhz" \
        capture-digital --firmware "$FW" --bitstream "$BS" \
        --sample-rate 1000000 --duration 1.0 --channels 0,1,2,3

    run_test "digital-8ch-1mhz" \
        capture-digital --firmware "$FW" --bitstream "$BS" \
        --sample-rate 1000000 --duration 1.0 --channels 0,1,2,3,4,5,6,7

    run_test "digital-8ch-5mhz" \
        capture-digital --firmware "$FW" --bitstream "$BS" \
        --sample-rate 5000000 --duration 1.0 --channels 0,1,2,3,4,5,6,7

    run_test "digital-8ch-10mhz" \
        capture-digital --firmware "$FW" --bitstream "$BS" \
        --sample-rate 10000000 --duration 1.0 --channels 0,1,2,3,4,5,6,7

    run_test "digital-2ch-25mhz" \
        capture-digital --firmware "$FW" --bitstream "$BS" \
        --sample-rate 25000000 --duration 1.0 --channels 0,1

    # --- Analog captures ---
    echo "==== Analog Captures ===="
    run_test "analog-1ch-1mhz" \
        capture-analog --firmware "$FW" --bitstream "$BS" \
        --sample-rate 1000000 --duration 1.0 --channels 0

    run_test "analog-2ch-1mhz" \
        capture-analog --firmware "$FW" --bitstream "$BS" \
        --sample-rate 1000000 --duration 1.0 --channels 0,1

    run_test "analog-4ch-1mhz" \
        capture-analog --firmware "$FW" --bitstream "$BS" \
        --sample-rate 1000000 --duration 1.0 --channels 0,1,2,3

    run_test "analog-2ch-5mhz" \
        capture-analog --firmware "$FW" --bitstream "$BS" \
        --sample-rate 5000000 --duration 1.0 --channels 0,1

    run_test "analog-1ch-10mhz" \
        capture-analog --firmware "$FW" --bitstream "$BS" \
        --sample-rate 10000000 --duration 1.0 --channels 0

    # --- Quick captures (mixed digital+analog) ---
    # capture-quick uses --output for raw file (not dir) and has no --format
    echo "==== Quick Captures (Mixed) ===="
    run_quick() {
        local name="$1"; shift
        echo "--- $name ---"
        local log="$OUT/$name.log"
        if "$CLI" -v "$@" --output "$OUT/$name.raw" >"$log" 2>&1; then
            tail -5 "$log"
            echo "  PASS"
            PASS=$((PASS + 1))
        else
            local rc=$?
            tail -10 "$log"
            echo "  FAIL (exit $rc)"
            FAIL=$((FAIL + 1))
        fi
        echo ""
    }

    run_quick "quick-digital-only" \
        capture-quick --firmware "$FW" --bitstream "$BS" \
        --duration 1.0 --digital-mask 0xff --analog-mask 0x00

    run_quick "quick-mixed-d8a2" \
        capture-quick --firmware "$FW" --bitstream "$BS" \
        --duration 1.0 --digital-mask 0xff --analog-mask 0x03

    run_quick "quick-mixed-d4a4" \
        capture-quick --firmware "$FW" --bitstream "$BS" \
        --duration 1.0 --digital-mask 0x0f --analog-mask 0x0f

    # --- Summary ---
    echo "==============================="
    echo "Capture Matrix Results:"
    echo "  PASS: $PASS"
    echo "  FAIL: $FAIL"
    echo "  Output: $OUT"
    echo "==============================="
    if [ "$FAIL" -gt 0 ]; then
        echo "Some captures failed. Check output above."
        exit 1
    fi

# Repeated idempotent capture test: full-init N times without power cycle
hw-capture-idempotent RUNS="3": _build-release
    #!/usr/bin/env bash
    set -euo pipefail
    CLI="target/release/saleae-cli"
    PASS=0; FAIL=0
    for i in $(seq 1 {{ RUNS }}); do
        echo "=== Run $i/{{ RUNS }} ==="
        if $CLI full-init --firmware {{ fw_path }} --bitstream {{ bs_path }} 2>&1; then
            PASS=$((PASS + 1))
            echo "  PASS"
        else
            FAIL=$((FAIL + 1))
            echo "  FAIL"
        fi
        echo ""
    done
    echo "==============================="
    echo "Idempotent capture test: $PASS/{{ RUNS }} passed, $FAIL failed"
    echo "==============================="
    [ "$FAIL" -eq 0 ]

# List connected Saleae devices with USB state
hw-list: _build-release
    target/release/saleae-cli list-devices

# Monitor USB bus for Saleae connect/disconnect events
hw-monitor: _build-release
    target/release/saleae-cli monitor

# Run cargo-fuzz on packet parsers (requires nightly)
fuzz TARGET="": _require-cargo
    @echo "NOTE: cargo-fuzz requires nightly Rust toolchain."
    @if [ -n "{{ TARGET }}" ]; then \
        cargo +nightly fuzz run {{ TARGET }}; \
    else \
        echo "Available fuzz targets:"; \
        cargo +nightly fuzz list 2>/dev/null || echo "(none yet)"; \
    fi
