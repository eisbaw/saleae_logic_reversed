let
  # Pin nixpkgs for reproducibility (NixOS 25.05, 2026-03-10)
  nixpkgs = fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/ac62194c3917d5f474c1a844b6fd6da2db95077d.tar.gz";
    sha256 = "0v6bd1xk8a2aal83karlvc853x44dg1n4nk08jg3dajqyy0s98np";
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

    # -- Ghidra MCP bridge (LaurieWired/GhidraMCP) --
    uv               # uv run for PEP 723 inline deps (requests, mcp)

    # -- Project tooling --
    just
  ];

  SALEAE_PATH = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted";
}
