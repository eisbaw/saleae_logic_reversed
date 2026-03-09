# Saleae Logic 2 Reverse Engineering - Project Commands
# Run all recipes inside nix-shell: nix-shell --run 'just <recipe>'

set dotenv-load := false

# Saleae binary locations (from shell.nix $SALEAE_PATH)
saleae_path     := env("SALEAE_PATH", "")
main_so         := saleae_path / "resources/linux-x64/libgraph_server_shared.so"
asar_file       := saleae_path / "resources/app.asar"

# Remote host where Saleae hardware is attached
usb_host        := "nvdk-msdlab-09"

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
    echo "=== Remote USB Host ({{ usb_host }}) ==="
    if ssh -o ConnectTimeout=3 {{ usb_host }} 'lsusb -d {{ saleae_vid }}: 2>/dev/null' &>/dev/null; then
        dev=$(ssh {{ usb_host }} 'lsusb -d {{ saleae_vid }}:')
        echo "  [ok]   Saleae device found: $dev"
    else
        echo "  [WARN] Cannot reach {{ usb_host }} or no Saleae device attached"
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

# Quick unlabeled USB capture for DURATION seconds (tcpdump, streams to local file)
usb-capture-quick DURATION="10":
    #!/usr/bin/env bash
    set -euo pipefail
    TIMESTAMP=$(date +%Y%m%d-%H%M%S)
    OUTFILE="re/captures/quick_${TIMESTAMP}.pcap"
    mkdir -p re/captures
    # Detect USB bus for Saleae device
    BUS=$(ssh {{ usb_host }} 'lsusb -d {{ saleae_vid }}:' | head -1 | sed 's/Bus \([0-9]*\).*/\1/' | sed 's/^0*//')
    echo "Capturing USB traffic on {{ usb_host }} bus $BUS for {{ DURATION }}s..."
    echo "Output: $OUTFILE"
    ssh {{ usb_host }} "sudo modprobe usbmon 2>/dev/null; \
        sudo timeout $(({{ DURATION }} + 2)) tcpdump -i usbmon${BUS} -w - 2>/dev/null" \
        > "$OUTFILE" 2>/dev/null || true
    echo "Capture complete: $(stat -c%s "$OUTFILE" 2>/dev/null || echo '?') bytes"
    echo "Analyze with: tshark -r $OUTFILE"

# List USB devices on remote host, highlighting Saleae devices
usb-list:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== USB devices on {{ usb_host }} ==="
    ssh {{ usb_host }} 'lsusb' | while read -r line; do
        if echo "$line" | grep -qiE '{{ saleae_vid }}|{{ saleae_vid_legacy }}'; then
            echo ">>> $line  <<< SALEAE"
        else
            echo "    $line"
        fi
    done

# Full USB descriptor dump for Saleae devices on remote host
usb-descriptors:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Saleae USB Descriptors ({{ usb_host }}) ==="
    ssh {{ usb_host }} 'lsusb -v -d {{ saleae_vid }}: 2>/dev/null' || true
    ssh {{ usb_host }} 'lsusb -v -d {{ saleae_vid_legacy }}: 2>/dev/null' || true

# Live USB monitor on remote host (tcpdump, raw output, Ctrl-C to stop)
usb-monitor:
    #!/usr/bin/env bash
    set -euo pipefail
    BUS=$(ssh {{ usb_host }} 'lsusb -d {{ saleae_vid }}:' | head -1 | sed 's/Bus \([0-9]*\).*/\1/' | sed 's/^0*//')
    echo "Live USB monitor on {{ usb_host }} bus $BUS (Ctrl-C to stop)..."
    ssh {{ usb_host }} "sudo modprobe usbmon 2>/dev/null; \
        sudo tcpdump -i usbmon${BUS} -nn -v"

# Check USB capture prerequisites on remote host
usb-check:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== USB Capture Prerequisites on {{ usb_host }} ==="
    echo ""
    echo "--- Device ---"
    ssh {{ usb_host }} 'lsusb -d {{ saleae_vid }}: 2>/dev/null' && echo "  [ok]" || echo "  [MISSING] No Saleae device"
    echo ""
    echo "--- tcpdump ---"
    ssh {{ usb_host }} 'which tcpdump && tcpdump --version 2>&1 | head -1' && echo "  [ok]" || echo "  [MISSING]"
    echo ""
    echo "--- tshark ---"
    ssh {{ usb_host }} 'which tshark 2>/dev/null' && echo "  [ok]" || echo "  [NOT INSTALLED] Install with: sudo dnf install wireshark-cli"
    echo ""
    echo "--- usbmon ---"
    ssh {{ usb_host }} 'sudo modprobe usbmon 2>/dev/null && ls /dev/usbmon* 2>/dev/null' && echo "  [ok]" || echo "  [ISSUE] Cannot load usbmon"
    echo ""
    echo "--- sudo ---"
    ssh {{ usb_host }} 'sudo -n true 2>/dev/null' && echo "  [ok] passwordless sudo" || echo "  [WARN] sudo requires password"

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

# Run clippy lints
clippy: _require-cargo
    cargo clippy --all-targets --all-features -- -D warnings

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

# Run cargo-fuzz on packet parsers (requires nightly)
fuzz TARGET="": _require-cargo
    @echo "NOTE: cargo-fuzz requires nightly Rust toolchain."
    @if [ -n "{{ TARGET }}" ]; then \
        cargo +nightly fuzz run {{ TARGET }}; \
    else \
        echo "Available fuzz targets:"; \
        cargo +nightly fuzz list 2>/dev/null || echo "(none yet)"; \
    fi
