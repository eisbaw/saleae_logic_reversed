#!/usr/bin/env python3
"""
Capture USB pcaps of Saleae Logic 2 GUI performing various capture configurations.

Prerequisites:
    1. Logic 2 GUI running with ENABLE_AUTOMATION=1
    2. logic2-automation 1.0.11 installed

Usage:
    # Start Logic 2 GUI first:
    ENABLE_AUTOMATION=1 saleae-logic-2 --disable-gpu &

    # Then run a scenario:
    nix-shell -p python312 python312Packages.grpcio python312Packages.protobuf tshark \\
        --run 'PYTHONPATH=/tmp/silly_logic2_env:$PYTHONPATH \\
               python3 scripts/capture_pcap.py --scenario digital_1mhz_8ch --verbose'

    # Run all scenarios:
    for s in digital_1mhz_8ch analog_1mhz_2ch mixed_12_5mhz_4d2a digital_25mhz_2ch mixed_asymmetric; do
        nix-shell -p ... --run '... --scenario $s --verbose'
    done

Scenarios (matching backlog tasks):
    digital_1mhz_8ch    - TASK-0221: Digital-only 1MHz 8ch
    analog_1mhz_2ch     - TASK-0222: Analog-only 1MHz 2ch
    mixed_12_5mhz_4d2a  - TASK-0223: Mixed 12.5MHz 4dig+2ana
    digital_25mhz_2ch   - TASK-0224: Digital-only 25MHz 2ch
    mixed_asymmetric    - TASK-0225: Mixed asymmetric rates
"""

import argparse
import os
import subprocess
import sys
import time
import logging as log
from datetime import datetime

from saleae import automation


SCENARIOS = {
    "digital_1mhz_8ch": {
        "description": "Digital-only 1MHz 8 channels",
        "device_config": automation.LogicDeviceConfiguration(
            enabled_digital_channels=[0, 1, 2, 3, 4, 5, 6, 7],
            digital_sample_rate=1_000_000,
            digital_threshold_volts=3.3,
        ),
        "duration": 2.0,
    },
    "analog_1_5mhz_2ch": {
        "description": "Analog-only 1.5625MHz 2 channels",
        "device_config": automation.LogicDeviceConfiguration(
            enabled_analog_channels=[0, 1],
            analog_sample_rate=1_562_500,
        ),
        "duration": 2.0,
    },
    "mixed_50m_12_5m_4d2a": {
        "description": "Mixed 50MHz digital + 12.5MHz analog, 4dig+2ana",
        "device_config": automation.LogicDeviceConfiguration(
            enabled_digital_channels=[0, 1, 2, 3],
            enabled_analog_channels=[0, 1],
            digital_sample_rate=50_000_000,
            analog_sample_rate=12_500_000,
            digital_threshold_volts=3.3,
        ),
        "duration": 2.0,
    },
    "digital_50mhz_2ch": {
        "description": "Digital-only 50MHz 2 channels",
        "device_config": automation.LogicDeviceConfiguration(
            enabled_digital_channels=[0, 1],
            digital_sample_rate=50_000_000,
            digital_threshold_volts=3.3,
        ),
        "duration": 2.0,
    },
    "mixed_125m_6_25m_4d2a": {
        "description": "Mixed 125MHz digital + 6.25MHz analog, 4dig+2ana",
        "device_config": automation.LogicDeviceConfiguration(
            enabled_digital_channels=[0, 1, 2, 3],
            enabled_analog_channels=[0, 1],
            digital_sample_rate=250_000_000,
            analog_sample_rate=6_250_000,
            digital_threshold_volts=3.3,
        ),
        "duration": 2.0,
    },
}


def find_usb_bus():
    """Find which USB bus the Saleae is on."""
    result = subprocess.run(["lsusb", "-d", "21a9:"], capture_output=True, text=True)
    for line in result.stdout.strip().split("\n"):
        if "21a9" in line:
            bus = line.split()[1]
            return int(bus)
    return None


def start_usbmon(bus, pcap_path):
    """Start tshark on usbmon for the given bus. Returns the subprocess."""
    usbmon_dev = f"usbmon{bus}"
    log.info(f"Starting usbmon capture on {usbmon_dev} -> {pcap_path}")
    subprocess.run(["sudo", "modprobe", "usbmon"], capture_output=True)
    proc = subprocess.Popen(
        ["tshark", "-i", usbmon_dev, "-w", pcap_path, "-q"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    time.sleep(2)
    return proc


def stop_usbmon(proc):
    """Stop the usbmon tshark process."""
    log.info("Stopping usbmon capture")
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def main():
    parser = argparse.ArgumentParser(description="Capture USB pcaps of Saleae GUI")
    parser.add_argument("--scenario", required=True, choices=list(SCENARIOS.keys()),
                        help="Capture scenario to run")
    parser.add_argument("--all", action="store_true",
                        help="Run all scenarios sequentially")
    parser.add_argument("--port", type=int, default=10430,
                        help="Logic 2 gRPC automation port")
    parser.add_argument("--output-dir", default="re/captures/gui_pcaps",
                        help="Output directory for pcap files")
    parser.add_argument("--no-usbmon", action="store_true",
                        help="Skip usbmon capture (just run the GUI capture)")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    if args.verbose:
        log.basicConfig(format="%(levelname)s: %(message)s", level=log.DEBUG)
    else:
        log.basicConfig(format="%(levelname)s: %(message)s", level=log.INFO)

    scenarios_to_run = list(SCENARIOS.keys()) if args.all else [args.scenario]

    for scenario_name in scenarios_to_run:
        run_scenario(scenario_name, args)


def run_scenario(scenario_name, args):
    scenario = SCENARIOS[scenario_name]
    log.info(f"=== Scenario: {scenario_name} - {scenario['description']} ===")

    os.makedirs(args.output_dir, exist_ok=True)
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")

    # Start usbmon
    usbmon_proc = None
    pcap_path = None
    if not args.no_usbmon:
        bus = find_usb_bus()
        if bus is None:
            log.error("No Saleae device found on USB")
            return
        pcap_path = os.path.join(args.output_dir, f"usbmon{bus}_{scenario_name}_{timestamp}.pcap")
        usbmon_proc = start_usbmon(bus, pcap_path)

    try:
        log.info(f"Connecting to Logic 2 gRPC on port {args.port}")
        manager = automation.Manager(port=args.port)

        # Use the proper get_devices() API (v1.0.11)
        devices = manager.get_devices()
        physical = [d for d in devices if not d.is_simulation]
        if not physical:
            log.error("No physical devices found")
            manager.close()
            return
        device_id = physical[0].device_id
        log.info(f"Using device: {device_id} ({physical[0].device_type})")

        capture_config = automation.CaptureConfiguration(
            capture_mode=automation.TimedCaptureMode(duration_seconds=scenario["duration"])
        )

        log.info(f"Starting capture: {scenario['description']}")
        with manager.start_capture(
            device_id=device_id,
            device_configuration=scenario["device_config"],
            capture_configuration=capture_config,
        ) as capture:
            log.info("Capturing... waiting for completion")
            capture.wait()
            log.info("Capture complete")

            sal_path = os.path.join(args.output_dir, f"{scenario_name}_{timestamp}.sal")
            capture.save_capture(filepath=os.path.abspath(sal_path))
            log.info(f"Saved .sal capture: {sal_path}")

        manager.close()

    finally:
        if usbmon_proc:
            time.sleep(1)
            stop_usbmon(usbmon_proc)
            if pcap_path:
                size = os.path.getsize(pcap_path) if os.path.exists(pcap_path) else 0
                log.info(f"Pcap saved: {pcap_path} ({size} bytes)")

    log.info(f"=== Done: {scenario_name} ===\n")


if __name__ == "__main__":
    main()
