# ethers / web3 sample scripts

Stand-alone Node.js scripts that exercise the Frontier-compatible JSON-RPC
through both `ethers` v6 and `web3` v4.

## Run

```bash
# 1. start a local dev chain
./target/release/solochain-template-node --dev --rpc-cors all --rpc-port 9944

# 2. install JS deps
cd tests/evm-tooling/scripts
npm install

# 3. native coin transfer (Alith → Baltathar)
node transfer-ethers.js

# 4. read-only RPC smoke check
node query-web3.js
```

Override the endpoint with `CRYPTO_NODE_RPC=http://host:port`.

`transfer-ethers.js` exits non-zero if the recipient balance delta does not
match the transferred amount; `query-web3.js` exits non-zero if the reported
chain id is not `32026`.
