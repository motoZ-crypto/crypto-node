const {
    connect, disconnectAll, pair, getUri, watchHeads,
} = require("../../js-scripts/lib");

async function run(_zombie, networkInfo, _args) {
    const api = await connect(networkInfo, "alice");
    try {
        const bob = await pair(getUri("bob"));
        const period = api.consts.validator.sessionPeriod.toNumber();
        
        console.log("📜", `  waiting for bob kick`);
        await watchHeads(api, async (header) => {
            const lock = await api.query.validator.validatorLocks(bob.address);
            if (lock.isSome && lock.unwrap().status.toString() === "Kicked") return true;
        }, period + 1);

        return 1;
    } catch (e) {
        console.error("📜", `  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll([api]);
    }
}

module.exports = { run };
