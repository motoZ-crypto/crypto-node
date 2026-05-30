const {
    connect, disconnectAll, pair, getUri, waitBlockAt,
    submitExtrinsic, sessionContainsValidator, waitForSessionRotations,
} = require("../../js-scripts/lib");

async function assertLockRejectedDuringCooldown(api, bob) {
    try {
        await submitExtrinsic(api, bob, api.tx.validator.lock());
    } catch (e) {
        if (e.message.includes("InCooldown")) {
            console.log("📜", `  got expected error: ${e.message}`);
            return;
        }
        throw new Error(`unexpected error: ${e.message}`);
    }
    throw new Error(`lock() succeeded but InCooldown was expected`);
}

async function waitForCooldownExpiry(api, bob) {
    const cd = await api.query.validator.rejoinCooldown(bob.address);
    if (cd.isNone) {
        throw new Error(`rejoinCooldown(bob) missing; ensure RejoinCooldownPeriod > SessionPeriod`);
    }
    console.log("📜", `  Kicked cooldownDeadline=#${cd.unwrap().toNumber()}`);
    const deadline = cd.unwrap().toNumber();

    const now = (await api.rpc.chain.getHeader()).number.toNumber();
    console.log("📜", `  current=#${now}, waiting for deadline=#${deadline}`);
    await waitBlockAt(api, deadline);
    console.log("📜", `  cooldown expired`);
}

async function assertCleanStateAfterCooldown(api, bob) {
    if (await sessionContainsValidator(api, bob.address)) {
        throw new Error(`bob still in session.validators() after kick+cooldown`);
    }
    const lock = await api.query.validator.validatorLocks(bob.address);
    if (lock.isSome) {
        throw new Error(`ValidatorLocks(bob) not cleared`);
    }
}

async function relockAndAssertBackInSession(api, bob) {
    await submitExtrinsic(api, bob, api.tx.validator.lock());
    console.log("📜", `  bob validator.lock() included`);
    console.log("📜", `  waiting two sessions`);
    await waitForSessionRotations(api, 2);
    if (!(await sessionContainsValidator(api, bob.address))) {
        throw new Error(`bob not in session.validators() after 2 sessions`);
    }
    console.log("📜", `  bob back in session.validators()`);
}

async function run(_zombie, networkInfo, _args) {
    const api = await connect(networkInfo, "alice");
    try {
        const bob = await pair(getUri("bob"));

        await assertLockRejectedDuringCooldown(api, bob);
        await waitForCooldownExpiry(api, bob);
        await assertCleanStateAfterCooldown(api, bob);
        await relockAndAssertBackInSession(api, bob);

        return 1;
    } catch (e) {
        console.error("📜", `  ${e.message}`);
        return 0;
    } finally {
        await disconnectAll([api]);
    }
}

module.exports = { run };
