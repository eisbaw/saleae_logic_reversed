let
  # Pin nixpkgs for reproducibility (NixOS 24.11, 2026-03-08)
  nixpkgs = fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/59e618d90c065f55ae48446f307e8c09565d5ab0.tar.gz";
    sha256 = "0ykyxbwl81m16yi94x6d3fvdbnwc8l3j9d8m2h2npyijzvbdizh7";
  };

  pkgs = import nixpkgs {};
in
pkgs.mkShell {
  buildInputs = with pkgs; [
    # -- Reverse engineering --
    ghidra
    binwalk
    wireshark-cli    # tshark, capinfos, etc. (no GUI)
    frida-tools
    ltrace
    strace

    # -- Binary / Electron analysis --
    nodePackages.asar
    binutils         # nm, readelf, objdump, strings
    python3
    xxd

    # -- Rust toolchain (stable) --
    rustc
    cargo
    clippy
    rustfmt
    cargo-nextest
    cargo-fuzz       # note: actual fuzzing requires nightly

    # -- nusb build deps (libudev for Linux hotplug) --
    pkg-config
    systemdLibs      # provides libudev.so + headers

    # -- Reference / comparison --
    libusb1          # headers for cross-referencing RE findings

    # -- Deployment --
    patchelf          # fix Nix rpath/interp for remote deploy

    # -- Project tooling --
    just
  ];

  SALEAE_PATH = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted";
}
