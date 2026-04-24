# Introduction

A blockchain project built using the Polkadot SDK.

## Getting Started

See [docs/how-to-build.md](docs/how-to-build.md) for instructions on building this blockchain node program in Rust.

## Mining

Mine and credit rewards to the given account:

```bash
./solochain-template-node --miner <YOUR_ADDRESS>
```

Mining never needs a private key. The node only puts the payout `AccountId` into the block header, 
so generate a keypair offline (e.g. with `subkey generate`) and pass only the SS58 address to the mining node. 
Keep the private key on a separate, offline machine.

If the address is invalid SS58 the node refuses to start.