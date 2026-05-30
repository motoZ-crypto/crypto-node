const {
    connect, disconnectAll, pair, getUri,
    sessionContainsValidator, waitForSessionRotations
} = require("../../js-scripts/lib");

async function run(_zombie, networkInfo, _args) {
    const api = await connect(networkInfo, "alice");
    try {

        const bob = await pair(getUri("bob"));
        if (!(await sessionContainsValidator(api, bob.address))) {
            console.error("📜", `  bob is not currently a validator`);
            return 0;
        }

        console.log("📜", `  waiting one session`);
        await waitForSessionRotations(api, 1);
        return 1;

    } catch (e) {
        console.error("📜", `  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll([api]);
    }
}

module.exports = { run };
