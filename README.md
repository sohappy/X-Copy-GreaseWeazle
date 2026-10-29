<p align="center">
  <img src="packaging/icon-128.png" width="128" alt="X-COPY Modern icon">
</p>

<h1 align="center">X-COPY Modernized version for Amiga by the Retro Computing Club (Adour) </h1>

<p align="center">🇬🇧 <b>English</b> · 🇫🇷 <a href="README.fr.md">Français</a></p>

<p align="center">
  <b>Retro Computing Club · RCC(A) Edition 1.0</b><br>
  Copy, format and write <b>Amiga</b> floppy disks with a <b>Greaseweazle</b>,<br>
  in a retro interface inspired by X-COPY.
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-1.95%2B-orange?logo=rust">
  <img alt="Platforms" src="https://img.shields.io/badge/macOS%20%7C%20Windows%20%7C%20Linux-blue">
  <img alt="Hardware" src="https://img.shields.io/badge/hardware-Greaseweazle-green">
  <img alt="Amiga" src="https://img.shields.io/badge/Amiga-DD%20880K%20%7C%20HD%201.76M-red">
  <img alt="License" src="https://img.shields.io/badge/license-GPL--3.0-blue">
  <a href="https://github.com/sohappy/x-copy-rcc/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/sohappy/x-copy-rcc/actions/workflows/ci.yml/badge.svg"></a>
</p>

<p align="center">
  <img src="docs/screenshot.png" width="900" alt="Main screen: BAMCOPY finished">
</p>

---

## Contents

- [Features](#features)
- [Download](#download)
- [Installation](#installation)
- [Getting started](#getting-started)
- [Usage](#usage)
- [Troubleshooting](#troubleshooting)
- [Building from source](#building-from-source)
- [Code layout](#code-layout)
- [Credits](#credits)
- [License](#license)

## Features

### COPY section

| Mode | What it does |
|---|---|
| `DOSCOPY` | AmigaDOS sector copy (ADF), with verified writes. |
| `DOSCOPY+` | Like DOSCOPY, with the recovery profile (5 revolutions, 8 retries, 2 head re-seeks) for damaged disks. |
| `BAMCOPY` | Reads cylinder 40 first (root block and bitmap), then **only the cylinders that hold files**. Much faster on a sparsely filled disk. The target disk is written in full, free tracks blank. If the bitmap is invalid, the whole disk is read. |
| `NIBBLE` | Copy at the magnetic flux level (SCP): keeps copy protections. |

**DEVICE** sets the copy direction:

| DEVICE | What it does |
|---|---|
| `DF>DF` | Disk to disk. If source and target are the same drive, X-COPY asks you to swap disks. |
| `DF>ADF` | Disk to image file. The volume name is offered as the file name (`Workbench Games.adf`). |
| `ADF>DF` | Image file (`.adf`, `.scp`, `.ipf`, `.hfe`…) to one or more disks. DD or HD is detected from the file size. |

### TOOLS section

| Mode | What it does |
|---|---|
| `FORMAT` | Blank AmigaDOS disk (OFS/FFS, DD/HD, volume name), written and verified. With DEVICE = `ADF`, saves a blank image instead. |
| `QFORMAT` | Quick format: every track is written, but without the verify pass. |
| `TRACK0` | Erases and rewrites **track 0 only** (blank boot block), with 8 verify retries. The rest of the disk is untouched; the disk no longer boots but its files stay readable. |
| `ERASE` | Erases the selected track range. |
| `SPEEDCHK` | Measures the source drive speed (RPM and deviation from 300). |
| `CLEAN` | Head cleaning with a cleaning disk. |
| `KICKDISK` | **Amiga 1000 Kickstart disk** from a 256K ROM image (like WriteKickDisk): block 0 `KICK`, blocks 1-512 the ROM, only cylinders 0-23 written (the disk does not need formatting first), verified. Accepts plain `.rom` files, byte-swapped dumps, 512K files holding the same ROM twice, and Amiga Forever encrypted ROMs when `rom.key` is in the same folder. Refuses 512K Kickstarts (2.0+), which an A1000 cannot load. With DEVICE = `ADF`, saves the disk as an ADF. DIRECTORY recognises Kickstart disks. |

### Buttons

| Button | Key | What it does |
|---|---|---|
| **START** | Enter | Starts the operation. You are asked to confirm before a disk is overwritten. |
| **REPEAT** | F2 | Writes the last image again to new disks, without reading the source again. Handy for a series of copies. |
| **STOP** | Esc | Stops cleanly; the drive is released. |
| **DISK INFO** | | Greaseweazle model, firmware, port and USB link. |
| **DIRECTORY** | | Volume, filesystem, boot block status and file list of the source disk or of an ADF. |
| **CHECKDISK** | | Quick read of the whole disk and a health map of its tracks. |
| **DEFAULT** | | Resets the copy options. |

### Also

- **Boot block check** after every sector-mode read: *blank*, *standard* (Workbench Install),
  *unknown code* (game loader, demo intro… or virus) or *suspect* (text typical of viruses:
  "VIRUS", "LAMER", "BANDIT"…). For an unknown or suspect boot block, X-COPY offers **REPLACE**
  (blank boot block in the image) or **KEEP**. This is not an antivirus: there is no signature
  database.
- **Several target drives** at once (lower lamps).
- **Partial copy** by cylinder range (START / END) and by side (SIDE).
- **Live track maps** for the upper and lower sides; hovering a cell shows its sectors.
- **Plain-language explanations** of `gw` errors (No Index, verify failure…).
- **Full log** of the `gw` commands: click the status line at the bottom.
- **X-COPY sounds**: the "Boing" sample when an operation completes, the error "Boing" when it fails
  (a short jingle when you stop it); can be turned off in SETUP.
- Settings are remembered between sessions.

## Download

Ready-to-run executables are on the
**[Releases](https://github.com/sohappy/x-copy-rcc/releases)** page:

| System | File | How to start |
|---|---|---|
| Windows 10/11 (x64) | `X-COPY-Modern-…-windows-x86_64.zip` | Unzip and run `X-COPY-Modern.exe`. |
| macOS 11+ (Apple Silicon and Intel) | `X-COPY-Modern-…-macos-universal.zip` | Unzip and drag **X-COPY Modern.app** to Applications. The app is not signed: the first time, **right-click → Open**. |
| Linux x86_64 | `X-COPY-Modern-…-linux-x86_64.tar.gz` | Extract, then run `./install.sh` (per-user install, with menu entry and icon). |

If macOS still refuses to open the app ("damaged"):

```bash
xattr -dr com.apple.quarantine "/Applications/X-COPY Modern.app"
```

## Installation

X-COPY Modern drives the Greaseweazle through Keir Fraser's **official `gw` host tools**, which
must be installed first.

### macOS

```bash
pip3 install greaseweazle
```

pip often installs `gw` in a folder that is not on the PATH, for example
`~/Library/Python/3.9/bin/gw`. X-COPY finds it by itself, but to use it in the Terminal too:

```bash
echo 'export PATH="$HOME/Library/Python/3.9/bin:$PATH"' >> ~/.zprofile
source ~/.zprofile
gw info
```

(Replace `3.9` with your Python version.) The Greaseweazle shows up as `/dev/cu.usbmodem…`.

The `NotOpenSSLWarning` printed by Apple's Python is harmless for floppy work.
To silence it: `pip3 install "urllib3<2"`.

### Windows

Download the official Greaseweazle tools archive and unzip it **next to `X-COPY-Modern.exe`**,
or into a `greaseweazle-…` folder in the same directory.

### Linux

```bash
pip install --user greaseweazle
sudo usermod -aG dialout $USER   # USB access, then log out and back in
```

### If `gw` is not found

Click **SETUP** (top right) and paste the full path of `gw` into *GW PROGRAM*, or click
**DETECT**. You can also start the program with the `XCOPY_GW` variable:

```bash
XCOPY_GW=$(which gw) ./xcopy-modern
```

## Getting started

1. Connect the Greaseweazle **and the drive's power supply** (4-pin connector: the Greaseweazle
   does not power the drive).
2. Start X-COPY Modern. At the top right, the GREASEWEAZLE line should read **GW READY**.
3. Click **DISK INFO**. The line changes to **DEVICE OK** and shows the model.
4. Check the wiring in **SETUP → BUS**:
   - **IBM PC** (default): PC drives on a twisted cable. DF0 = drive A (after the twist),
     DF1 = drive B.
   - **SHUGART**: original Amiga drives or a straight cable. DF0 to DF3 = units 0 to 3.
5. Insert an Amiga disk and click **CHECKDISK**: it only reads, so it is safe.

## Usage

**Copy a disk with a single drive**
COPY → `DOSCOPY` (or `BAMCOPY`), DEVICE `DF>DF`, source lamp DF0 and target lamp DF0, then **START**.
X-COPY reads the source, then asks you to insert the target disk. **REPEAT** makes one more copy.

**Save a disk as an ADF**
DEVICE `DF>ADF`, then **START**. The volume name is offered as the file name.

**Write an ADF to a disk**
DEVICE `ADF>DF`, choose the target drive(s), **START**, then pick the file.

**Format**
TOOLS → `FORMAT`, click NAME to set the volume name, choose FS (OFS for Kickstart 1.x,
FFS for 2.0 and later) and DENS, then **START**.

### Reading the track maps

| Cell | Meaning |
|---|---|
| Green `O` | Track read, or written and verified. |
| Yellow `R` | Recovered after retries: worn surface. |
| Red `1`…`9` | Unreadable track: number of missing sectors (`X` = more than 9). |
| Orange `?` | Retry in progress. |
| Blue `U` | Empty (unformatted) track. |
| Cyan `F` | Raw flux captured (NIBBLE). |
| Blinking white | Track in progress. |
| Dark cell | Track not part of the operation (for example, not read by BAMCOPY). |

**Upper** side = head 1, **lower** side = head 0 (Amiga convention).

### Controls

- START / END arrows: click = 1 cylinder, right-click = 10, or use the mouse wheel.
- Options with an orange dot: click to cycle through the values. Right-click on the mode cycles backwards.
- Upper lamps = source drive, lower lamps = target drives (several allowed).

## Troubleshooting

| Message or symptom | Likely cause and fix |
|---|---|
| `GW NOT FOUND` | `gw` tools missing or not on the PATH: see [If `gw` is not found](#if-gw-is-not-found). |
| `GetFluxStatus: No Index` | The drive does not see the disk turning. Check the drive's power, that the disk is fully inserted, the bus (IBM or Shugart) and the selected drive. Test each position with `gw rpm --drive A`, `B`, `0`, `1`. |
| `Failed to verify Track 0.x` on many disks | Worn track 0 (the busiest track on an Amiga disk), an **HD** disk used as DD (cover the density hole with opaque tape), an old format getting in the way (try `TRACK0`, or `ERASE` then FORMAT), or dirty heads (`CLEAN`). |
| Red cells after FORMAT + CHECKDISK | Damaged surface: discard the disk. |
| The drive LED stays on | Cable plugged in upside down. |

**Is a disk dead?** FORMAT (not QFORMAT), then CHECKDISK. All green `O`: good. A few `R`: worn.
Red cells: dead on those tracks. Test a known-good disk first, to rule out the drive.

## Building from source

You need **Rust 1.95 or later**, installed with [rustup](https://rustup.rs). An existing Rust
install is updated with `rustup update`.

### Windows

1. Install the **Visual Studio Build Tools** with the "Desktop development with C++" workload
   (the rustup installer offers it).
2. Install Rust: download and run `rustup-init.exe` from [rustup.rs](https://rustup.rs).
3. In PowerShell:

```powershell
git clone https://github.com/sohappy/x-copy-rcc.git
cd x-copy-rcc
cargo build --release -p xcopy-modern
.\target\release\xcopy-modern.exe
```

The icon is embedded in the executable by `build.rs`.

### macOS

```bash
xcode-select --install                                           # Apple tools (once)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust (once)
git clone https://github.com/sohappy/x-copy-rcc.git
cd x-copy-rcc
cargo build --release -p xcopy-modern
./target/release/xcopy-modern
```

To get an **X-COPY Modern.app** application with its icon (in `dist/`):

```bash
bash packaging/package.sh macos
```

Universal build (Apple Silicon + Intel):

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release -p xcopy-modern --target aarch64-apple-darwin
cargo build --release -p xcopy-modern --target x86_64-apple-darwin
mkdir -p target/universal
lipo -create -output target/universal/xcopy-modern \
  target/aarch64-apple-darwin/release/xcopy-modern target/x86_64-apple-darwin/release/xcopy-modern
bash packaging/package.sh macos-universal
```

### Linux

System libraries needed by the GUI (Debian / Ubuntu):

```bash
sudo apt install build-essential pkg-config libxkbcommon-dev libgl1-mesa-dev libwayland-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
git clone https://github.com/sohappy/x-copy-rcc.git
cd x-copy-rcc
cargo build --release -p xcopy-modern
./target/release/xcopy-modern
```

Fedora: `sudo dnf install gcc pkgconf libxkbcommon-devel mesa-libGL-devel wayland-devel libxcb-devel`.

To install the program with a menu entry and its icon:

```bash
bash packaging/package.sh linux-x86_64
tar xzf dist/X-COPY-Modern-dev-linux-x86_64.tar.gz
./X-COPY-Modern-dev-linux-x86_64/install.sh
```

### Tests

```bash
cargo test --workspace
```

### Automatic builds (GitHub Actions)

* **`ci.yml`**: on every push, builds and tests on Linux, Windows and macOS.
* **`release.yml`**: on every `v…` tag, builds the three versions (Windows `.zip`, universal macOS
  `.app`, Linux `.tar.gz`) and publishes a GitHub release:

```bash
git tag v1.0.0
git push origin v1.0.0
```

From the **Actions → Release → Run workflow** tab, the three versions are built and can be
downloaded as artifacts, without publishing a release.

### Trying it without hardware

`tools/fake-gw.py` simulates `gw`: an Amiga disk with 3 damaged tracks, a write with one verify
retry, `info`, `rpm`…

```bash
XCOPY_GW=$PWD/tools/fake-gw.py ./target/release/xcopy-modern

# The "disk" in the drive is this ADF (to test BAMCOPY, DIRECTORY, the boot block check…)
FAKE_ADF=my_disk.adf XCOPY_GW=$PWD/tools/fake-gw.py ./target/release/xcopy-modern

# Simulates a verify failure while writing
FAKE_VERIFY_FAIL=1 XCOPY_GW=$PWD/tools/fake-gw.py ./target/release/xcopy-modern
```

## Code layout

```
Cargo.toml           Workspace: every dependency declared once ([workspace.dependencies])
crates/
├── hgf-core/        Greaseweazle and floppy-disk core (no GUI, unit-tested)
│   ├── gw/          Talking to gw: commands (Action), runner, Spawner trait, output parser, locate, ports
│   ├── disk/        Track sets, live disk map, formats and presets, image files, naming
│   ├── fs/          Filesystems: BlankDisk trait, byte helpers, FAT12, CBM DOS, Acorn DFS, Apple II
│   │   └── amigados/  checksums and layout, format (blank disks), explore (ADF explorer), analyse (bitmap, boot block), kickstart (A1000 Kickstart disks)
│   └── diag/        Drive health, plain-language hints, Amiga write compatibility
└── xcopy/src/
    ├── main.rs      Window (eframe/egui), icon
    ├── app/         The X-COPY application
    │   ├── mod.rs     state, click dispatcher, eframe loop
    │   ├── ops.rs     operations started by the buttons (copy, format, …) as step lists
    │   ├── dialogs.rs file dialogs (non-blocking) and requesters
    │   └── screen.rs  drawing of the screen and requesters
    ├── engine.rs    Step queue (gw, confirmation, disk swap, checks), testable with a scripted Spawner
    ├── ui/          Retro rendering: font.rs (8 × 8 bitmap font), gfx.rs (scaled screen, panels, lamps, logo)
    ├── sound.rs     End sounds (X-COPY samples, synthesised abort jingle), played by the system player
    └── settings.rs  Saved options
packaging/           Icons (PNG 16 to 512 px, .ico, .icns), package.sh, macOS bundle, Linux install
tools/fake-gw.py     gw simulator for testing
.github/workflows/   ci.yml (fmt + clippy, tests on 3 systems), release.yml (executables and release)
```

Two traits carry the abstractions that are actually shared:

* **`fs::BlankDisk`**: every formatter (AmigaDOS, FAT12, CBM, DFS, Apple II) validates its input,
  names its `gw` format and file extension and builds its image through the same interface.
* **`gw::spawn::Spawner` / `JobControl`**: the engine starts `gw` through this interface;
  `ProcessSpawner` runs the real program, `ScriptedSpawner` replays captured output, so the copy
  logic (confirmation, read, bad tracks, write, failures, stop) is unit-tested without hardware.

Closed sets of cases (gw actions, engine steps, track states) are Rust `enum`s with exhaustive
`match`: adding a case does not compile until every place handles it.

## Credits

- **[Greaseweazle](https://github.com/keirf/greaseweazle)** and its `gw` tools: Keir Fraser.
- **hgf-core**: the HappyGreaseFloppy engine, included in `crates/hgf-core`.
- **[egui / eframe](https://github.com/emilk/egui)**: GUI.
- **[font8x8](https://crates.io/crates/font8x8)**: 8 × 8 bitmap glyphs (public domain).
- **[rfd](https://crates.io/crates/rfd)**: native file dialogs.

The interface pays tribute to **X-COPY Professional**, the 1990s Amiga disk copier. The logo and
graphics are original creations; the original logo, its authors' credits, the mascot and the Amiga
logo are not reproduced. The two "Boing" sounds in `crates/xcopy/assets/sounds/` come from X-COPY
and remain the property of their authors: they are not covered by this project's GPL license. X-COPY and Amiga are trademarks of their respective owners.
This project is affiliated neither with them nor with the Greaseweazle project.

## License

Copyright © 2026 Retro Computing Club (RCC).

X-COPY Modern is free software: you can redistribute it and/or modify it under the terms of the
**GNU General Public License version 3**, or (at your option) any later version, as published by
the Free Software Foundation (`GPL-3.0-or-later`).

This program is distributed in the hope that it will be useful, but **without any warranty**;
without even the implied warranty of merchantability or fitness for a particular purpose. The full
text is in the [`LICENSE`](LICENSE) file.

The libraries used keep their own licenses, all compatible with GPL-3.0 (MIT, Apache-2.0, Zlib,
BSD, ISC); the `font8x8` glyphs are public domain. The Greaseweazle tools (`gw`) are a separate
program, called by X-COPY Modern and not included.

> ⚠️ Writing, formatting and erasing **destroy** the contents of the target disks.
> Make an ADF image of your precious disks before doing anything with them.
