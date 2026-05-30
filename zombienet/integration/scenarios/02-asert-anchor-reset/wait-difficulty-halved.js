const { connect, sleep } = require("../../js-scripts/lib");

async function run(_zombie, networkInfo) {
    const api = await connect(networkInfo, "eve");
    try {
        const breakThresholdSecs = api.consts.difficulty.breakThresholdSecs.toBigInt();
        const targetBlockTimeSecs = api.consts.difficulty.targetBlockTime.toBigInt();
        const lastBlockTsSecs = (await api.query.timestamp.now()).toBigInt() / 1000n;
        const targetSecs = lastBlockTsSecs + breakThresholdSecs + targetBlockTimeSecs;
        const nowSecs = BigInt(Math.floor(Date.now() / 1000));
        const waitMs = nowSecs >= targetSecs ? 0 : Number(targetSecs - nowSecs) * 1000;
        console.log("📜", `  breakThreshold=${breakThresholdSecs}s lastBlockTs=${lastBlockTsSecs} target=${targetSecs} nowSecs=${nowSecs} waitMs=${waitMs}`);
        await sleep(waitMs);

        const checkNowSecs = BigInt(Math.floor(Date.now() / 1000));
        const current = (await api.query.difficulty.currentDifficulty()).toBigInt();
        const realtime = (await api.call.difficultyApi.realtimeDifficulty(checkNowSecs)).toBigInt();
        console.log("📜", `  at target now=${checkNowSecs} currentDifficulty=${current} realtime=${realtime}`);
        if (realtime * 2n > current) {
            console.error("📜", `  difficulty did not decay to <= 1/2 by anchor-reset time`);
            return 0;
        }
        return 1;
    } finally {
        await api.disconnect();
    }
}

module.exports = { run };
