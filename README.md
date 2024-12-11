# Namada

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](./LICENSE)
![CI Status](https://github.com/anoma/namada/actions/workflows/ci.yml/badge.svg?branch=main)

## Overview

[Namada](http://namada.net) is a Proof-of-Stake L1 for interchain asset-agnostic privacy. Namada uses CometBFT
consensus and enables multi-asset shielded transfers for any native
or non-native asset. Namada features full IBC protocol support,
a natively integrated Ethereum bridge, a modern proof-of-stake
system with automatic reward compounding and cubic slashing, and a
stake-weighted governance signalling mechanism. Users of shielded
transfers are rewarded for their contributions to the privacy set in
the form of native protocol tokens. A multi-asset shielded transfer
wallet is provided in order to facilitate safe and private user
interaction with the protocol.

<<<<<<< HEAD
* Blogpost: [Introducing Namada: Interchain Asset-agnostic Privacy](https://blog.namada.net/introducing-namada-interchain-asset-agnostic-privacy/)
=======
* Blogpost: [Introducing Namada: Interchain Asset-agnostic Privacy](https://namada.net/blog/introducing-namada-multichain-asset-agnostic-data-protection)
>>>>>>> 52d0ebbd7c (Revert "ci: minors")

## 📓 Docs

* [User guides](https://docs.namada.net/)
* [Specs](https://specs.namada.net/)
* Rust docs can be built with `cargo doc --open` (add `--no-deps` to only build docs for local crates)

## Warning

> Here lay dragons: this codebase is still experimental, try at your own risk!

## 💾 Installing

There is a single command to build and install Namada executables from source (the node, the client and the wallet). This command will also verify that a compatible version of [CometBFT](#dependencies) is available and if not, attempt to install it. Note that currently at least 16GB RAM is needed to build from source.

```shell
make install
```

After installation, the main `namada` executable will be available on path.

<<<<<<< HEAD
To find how to use it, check out the [User Guide section of the docs](https://docs.namada.net/user-guide/index.html).

For more detailed instructions and more install options, see the [Install
section](https://docs.namada.net/user-guide/install/index.html) of the User
=======
To find how to use it, check out the [User Guide section of the docs](https://docs.namada.net/users).

For more detailed instructions and more install options, see the [Install
section](https://docs.namada.net/introduction/install) of the User
>>>>>>> 52d0ebbd7c (Revert "ci: minors")
Guide.

## ⚙️ Development

```shell
# Build the provided validity predicate and transaction wasm modules
make build-wasm-scripts-docker
```

<<<<<<< HEAD
### Before submitting a PR, pls make sure to run the following
=======
### Before submitting a PR, please make sure to run the following
>>>>>>> 52d0ebbd7c (Revert "ci: minors")

```shell
# Format the code
make fmt

# Lint the code
make clippy
```

## 🧾 Logging

To change the log level, set `NAMADA_LOG` environment variable to one of:

* `error`
* `warn`
* `info`
* `debug`
* `trace`

<<<<<<< HEAD
The default is set to `info` for all the modules, expect for CometBFT ABCI, which has a lot of `debug` logging.
=======
The default is set to `info` for all the modules, except for CometBFT ABCI, which has a lot of `debug` logging.
>>>>>>> 52d0ebbd7c (Revert "ci: minors")

For more fine-grained logging levels settings, please refer to the [tracing subscriber docs](https://docs.rs/tracing-subscriber/0.2.18/tracing_subscriber/struct.EnvFilter.html#directives) for more information.

To switch on logging in tests that use `#[test]` macro from `test_log::test`, use `RUST_LOG` with e.g. `RUST_LOG=info cargo test -- --nocapture`.

## How to contribute

Please see the [contributing page](./CONTRIBUTING.md).

### Dependencies

<<<<<<< HEAD
The ledger currently requires [CometBFT v0.37.11](https://github.com/cometbft/cometbft/releases/tag/v0.37.11) is installed and available on path. This can be achieved through following [these instructions](https://github.com/cometbft/cometbft/blob/main/docs/tutorials/install.md)
=======
The ledger currently requires [CometBFT v0.37.11](https://github.com/cometbft/cometbft/releases/tag/v0.37.11) is installed and available on path. This can be achieved through following [these instructions](https://github.com/cometbft/cometbft/blob/main/docs/tutorials/install.md).
>>>>>>> 52d0ebbd7c (Revert "ci: minors")

#### Hermes

We maintain a fork of [hermes](https://github.com/heliaxdev/hermes) that adds support for Namada.

Compatibility table with Namada:

<<<<<<< HEAD
| Namada | Hermes |
| ----------- | ----------- |
| v0.44.0 | 1.10.4-namada-beta17-rc2 |
=======
| Namada binaries | Hermes |
| ----------- | ----------- |
| v1.0.0 | [1.10.4-namada-beta17-rc2](https://github.com/heliaxdev/hermes/releases/tag/v1.10.4-namada-beta17-rc2) |
>>>>>>> 52d0ebbd7c (Revert "ci: minors")
