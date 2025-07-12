# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is **StarryOS** (FreeBSD), a monolithic kernel based on [ArceOS](https://github.com/arceos-org/arceos). It's designed for the 2025 OS competition and supports multiple architectures: x86_64, riscv64, aarch64, and loongarch64.

### Architecture

The project follows a modular monolithic kernel design:

- **Root**: Main kernel entry point and top-level coordination
- **api/**: System call interface and user-space API (starry-api crate)
- **core/**: Core kernel functionality including memory management, process management, and synchronization (starry-core crate)
- **arceos/**: Base OS framework providing hardware abstraction, drivers, and basic services
- **apps/**: User-space test applications and testcases

Key architectural components:
- System calls are implemented in `src/syscall.rs` with detailed implementations in `api/src/imp/`
- Process management through `axprocess` and task scheduling via `axtask`
- Memory management using `axmm` with page tables and address spaces
- File system support via `axfs` with multiple backends (ext4, fat32, ramfs, devfs)
- Network stack through `axnet` with socket API

## Build Commands

### Dependencies Setup
```bash
# Install build dependencies
cargo install cargo-binutils axconfig-gen
sudo apt install libclang-dev cmake dosfstools build-essential

# Get ArceOS base repository
./scripts/get_deps.sh

# Download toolchains (see README.md for URLs and setup)
export PATH=[toolchain-paths]:$PATH
```

### Build and Run
```bash
# Build user applications for a testcase
make ARCH=<arch> AX_TESTCASE=<testcase> user_apps

# Generate configuration (required when changing architecture)
make ARCH=<arch> defconfig

# Build kernel
make ARCH=<arch> AX_TESTCASE=<testcase> build

# Run kernel with QEMU
make ARCH=<arch> AX_TESTCASE=<testcase> BLK=y NET=y FEATURES=fp_simd LOG=<level> run

# Run just the kernel (without rebuilding)
make ARCH=<arch> AX_TESTCASE=<testcase> justrun
```

Where:
- `<arch>`: `x86_64`, `riscv64`, `aarch64`, `loongarch64`
- `<testcase>`: `nimbos`, `libc`, `oscomp`, `junior`, `custom`
- `<level>`: `off`, `error`, `warn`, `info`, `debug`, `trace`

### Testing
```bash
# Run automated tests for nimbos and libc
make test

# Run OS competition testcases
make oscomp_run ARCH=<arch>

# Build for OS competition submission
make oscomp_build
```

### Linting and Documentation
```bash
# Run clippy linter
make clippy

# Generate documentation
make doc

# Clean build artifacts
make clean
```

## Development Workflow

### Adding System Calls
1. Define system call number in `api/src/imp/sys.rs`
2. Implement in appropriate module under `api/src/imp/`
3. Add entry point in main syscall dispatcher `src/syscall.rs`
4. Test with user applications in `apps/`

### Adding Test Cases
1. Create directory under `apps/` with `Makefile` and `testcase_list`
2. For C programs: add source in `c/` subdirectory
3. For binaries: place executable and update `testcase_list`
4. Build disk image: `./build_img.sh -a <arch> -file apps/<testcase>`

### File System Development
- Use `./build_img.sh` to create disk images with test files
- Support for ext4 (via lwext4_rs) and fat32 file systems
- Virtual file systems in `arceos/modules/axfs/`

### Architecture-Specific Code
- Platform code in `arceos/modules/axhal/src/platform/`
- Architecture-specific implementations in `arceos/modules/axhal/src/arch/`
- Target configurations in `configs/<arch>.toml`

## Key Files and Modules

- `src/main.rs`: Kernel entry point, executes testcases
- `src/syscall.rs`: System call dispatcher
- `api/src/imp/`: System call implementations organized by category
- `core/src/`: Core kernel services (mm, task, time, futex)
- `arceos/modules/`: Hardware abstraction and device drivers
- `Makefile`: Build system with architecture and feature configuration
- `scripts/make/oscomp.mk`: OS competition specific build targets

## Important Notes

- Always run `make defconfig` when changing architecture
- Use `ACCEL=n` if KVM acceleration causes issues
- Enable `fp_simd` feature for floating-point/SIMD support in libc tests
- The `lwext4_rs` feature enables ext4 file system support
- Log levels affect both build and runtime output verbosity