# Build

This guide explains how to build and run the blockchain node using Rust.  
It covers the required dependencies, environment setup, and step-by-step build instructions.

## Build Dependencies

### Ubuntu/Debian

Use a terminal shell to execute the following commands:

```bash
sudo apt update
sudo apt install -y git clang curl libssl-dev llvm libclang-dev libudev-dev make protobuf-compiler
```

### Arch Linux

Run these commands from a terminal:

```bash
pacman -Syu --needed --noconfirm git curl clang llvm openssl protobuf make
```

### Fedora

Run these commands from a terminal:

```bash
sudo dnf update
sudo dnf install git curl clang llvm-devel openssl-devel systemd-devel make protobuf-compiler
```

### OpenSUSE

Run these commands from a terminal:

```bash
sudo zypper install git curl clang llvm-devel openssl-devel libudev-devel make protobuf
```

### macOS

> **Apple M1/M2 ARM** If you have an Apple M1/M2 ARM system on a chip, make sure that you have Apple Rosetta 2 installed
> through `softwareupdate --install-rosetta`. This is only needed to run the `protoc` tool during the build. The build
> itself and the target binaries would remain native.

Open the Terminal application and execute the following commands:

```bash
# Install Homebrew if necessary https://brew.sh/
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/master/install.sh)"

# Make sure Homebrew is up-to-date, install dependencies
brew update
brew install openssl protobuf
```

### Windows

**_PLEASE NOTE:_** Native Windows development of Substrate is _not_ very well supported! It is _highly_
recommended to use [Windows Subsystem Linux](https://docs.microsoft.com/en-us/windows/wsl/install)
(WSL2) and follow the instructions for [Ubuntu/Debian](#ubuntudebian).

---

## Rust Developer Environment

### 1. Install rustup

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### 2. Install the stable toolchain and add the WASM target

```bash
rustup default stable
rustup update
rustup target add wasm32v1-none
rustup component add rust-src
```

> **Note**: Since Rust 1.84, `wasm32v1-none` is the recommended target over the legacy `wasm32-unknown-unknown`.
> `wasm32v1-none` is designed for bare-metal WASM environments without OS assumptions, making it a better fit for blockchain runtimes.
> **A nightly toolchain is no longer required.**

### 3. Verify your setup

```bash
rustup show
```

Expected output:

```text
Default host: x86_64-unknown-linux-gnu

installed targets for active toolchain
--------------------------------------
wasm32v1-none
x86_64-unknown-linux-gnu

active toolchain
----------------
stable-x86_64-unknown-linux-gnu (default)
rustc 1.84.0 (...)
```

---

## Building the Project

```bash
cargo build
```

The first build compiles all dependencies and may take 20–60 minutes.

### Minimum Hardware Requirements

| Component | Minimum |
|-----------|---------|
| RAM       | 32 GB   |
| Disk      | 64 GB   |
