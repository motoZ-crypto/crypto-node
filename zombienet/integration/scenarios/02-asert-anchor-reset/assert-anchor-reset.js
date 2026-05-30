const { connect, waitBlock } = require("../../js-scripts/lib");

async function run(_zombie, networkInfo) {
    const api = await connect(networkInfo, "eve");
    try {
        const breakThresholdSecs = api.consts.difficulty.breakThresholdSecs.toBigInt();

        await waitBlock(api, 2);

        let hash = (await api.rpc.chain.getHeader()).hash;
        let recoveryNum = null;
        let recoveryHash = null;
        let recoveryTsSecs = null;
        let nextHash = null; // hash of block N+1

        let cursorHash = hash;
        let cursorChildHash = null;
        while (true) {
            const header = await api.rpc.chain.getHeader(cursorHash);
            const num = header.number.toNumber();
            if (num === 0) break;

            const tsSecs = (await (await api.at(cursorHash)).query.timestamp.now()).toBigInt() / 1000n;
            const parentTsSecs = (await (await api.at(header.parentHash)).query.timestamp.now()).toBigInt() / 1000n;

            if (tsSecs - parentTsSecs > breakThresholdSecs) {
                recoveryNum = num;
                recoveryHash = cursorHash;
                recoveryTsSecs = tsSecs;
                nextHash = cursorChildHash;
                break;
            }
            cursorChildHash = cursorHash;
            cursorHash = header.parentHash;
        }

        if (recoveryNum === null) {
            console.error("📜", "  no recovery block found (searched back to genesis)");
            return 0;
        }
        if (nextHash === null) {
            console.error("📜", `  recovery block #${recoveryNum} has no child yet; need to wait for block N+1`);
            return 0;
        }
        console.log("📜", `  recovery block: #${recoveryNum} ts=${recoveryTsSecs}`);

        // Read anchor storage at block N+1's state.
        const apiAtNext = await api.at(nextHash);
        const [anchorHeightRaw, anchorTsRaw, anchorTargetRaw, recoveryAnchorTargetRaw] = await Promise.all([
            apiAtNext.query.difficulty.anchorHeight(),
            apiAtNext.query.difficulty.anchorTimestamp(),
            apiAtNext.query.difficulty.anchorTarget(),
            (await api.at(recoveryHash)).query.difficulty.anchorTarget(),
        ]);
        const anchorHeight = Number(anchorHeightRaw.toBigInt());
        const anchorTsSecs = anchorTsRaw.toBigInt();
        const anchorTarget = anchorTargetRaw.toBigInt();
        const recoveryAnchorTarget = recoveryAnchorTargetRaw.toBigInt();
        console.log("📜", `  anchor@#${recoveryNum + 1}: height=${anchorHeight} ts=${anchorTsSecs} target=${anchorTarget}`);

        if (anchorHeight !== recoveryNum) {
            console.error("📜", `  anchor.height mismatch: got ${anchorHeight} expected ${recoveryNum}`);
            return 0;
        }
        if (anchorTsSecs !== recoveryTsSecs) {
            console.error("📜", `  anchor.timestamp mismatch: got ${anchorTsSecs} expected ${recoveryTsSecs}`);
            return 0;
        }
        if (anchorTarget !== recoveryAnchorTarget) {
            console.error("📜", `  anchor.target mismatch: got ${anchorTarget} expected ${recoveryAnchorTarget} (recovery block's own anchorTarget)`);
            return 0;
        }
        return 1;
    } finally {
        await api.disconnect();
    }
}

module.exports = { run };
