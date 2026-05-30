const {
    connect, disconnectAll, pair, getUri, watchHeads, waitBlockAt,
    submitExtrinsic, sessionContainsValidator, waitForSessionRotations,
    keyring,
} = require("../../js-scripts/lib");
const { u8aConcat, hexToU8a, u8aToHex, stringToU8a } = require("@polkadot/util");
const { blake2AsU8a } = require("@polkadot/util-crypto");

// Construct a synthetic GRANDPA prevote equivocation by Bob and submit
// `grandpa.reportEquivocationUnsigned`. Two prevotes are signed for the
// same (round, set_id) but distinct (target_hash, target_number). The
// runtime's `EquivocationReportSystem` validates the proof, the
// `GrandpaOffenceReporter` adapter then calls
// `pallet_validator::note_equivocation(bob)` which flips bob's lock to
// `Kicked` and emits `ValidatorKicked { reason: Equivocation }`.
async function reportBobEquivocation(api) {
    const kr = await keyring();
    // The GRANDPA authority key is the well-known `//Bob` ed25519 derivation.
    const ed = new (require("@polkadot/keyring").Keyring)({ type: "ed25519" });
    const bobGrandpa = ed.addFromUri("//Bob");
    const identity = bobGrandpa.publicKey; // 32-byte ed25519 pubkey

    const setId = (await api.query.grandpa.currentSetId()).toBigInt();
    const round = 1n;

    // Two distinct prevote targets. Hashes need not correspond to real
    // blocks: the pallet only verifies the signatures over the encoded
    // payload and that the two messages differ.
    const targetA = {
        targetHash: u8aToHex(blake2AsU8a(stringToU8a("equivocation-target-a"))),
        targetNumber: 1,
    };
    const targetB = {
        targetHash: u8aToHex(blake2AsU8a(stringToU8a("equivocation-target-b"))),
        targetNumber: 2,
    };

    // Localized signing payload: (Message::Prevote(prevote), round, set_id).encode()
    // Message enum tag for Prevote variant is 0.
    const encodePrevote = (t) => u8aConcat(
        hexToU8a(t.targetHash),
        api.createType("u32", t.targetNumber).toU8a(),
    );
    const signingPayload = (t) => u8aConcat(
        new Uint8Array([0x00]),
        encodePrevote(t),
        api.createType("u64", round).toU8a(),
        api.createType("u64", setId).toU8a(),
    );

    const sigA = bobGrandpa.sign(signingPayload(targetA));
    const sigB = bobGrandpa.sign(signingPayload(targetB));

    const equivocationProof = {
        setId,
        equivocation: {
            Prevote: {
                roundNumber: round,
                identity: u8aToHex(identity),
                first: [targetA, u8aToHex(sigA)],
                second: [targetB, u8aToHex(sigB)],
            },
        },
    };

    const opaque = await api.call.grandpaApi.generateKeyOwnershipProof(setId, u8aToHex(identity));
    if (opaque.isNone) {
        throw new Error(`generateKeyOwnershipProof returned None for bob in set_id=${setId}`);
    }
    // polkadot-js doesn't fully strip the runtime API envelope for `Option<OpaqueKeyOwnershipProof>`:
    // `opaque.unwrap().toU8a(true)` still includes the Option Some-tag and Compact<u32> length prefix.
    // We strip those 3 leading bytes to recover the raw MembershipProof bytes.
    const envelopeBytes = opaque.unwrap().toU8a(true);
    if (envelopeBytes[0] !== 0x01) {
        throw new Error(`unexpected envelope first byte 0x${envelopeBytes[0].toString(16)} (expected 0x01 Some-tag)`);
    }
    const opaqueInnerBytes = envelopeBytes.slice(3);
    const sessionFromBytes = new DataView(opaqueInnerBytes.buffer, opaqueInnerBytes.byteOffset, 4).getUint32(0, true);
    const validatorCountFromBytes = new DataView(opaqueInnerBytes.buffer, opaqueInnerBytes.byteOffset + opaqueInnerBytes.length - 4, 4).getUint32(0, true);
    console.log("📜", `  membership proof: session=${sessionFromBytes} validator_count=${validatorCountFromBytes} bytes=${opaqueInnerBytes.length}`);
    const keyOwnerProof = api.createType("SpSessionMembershipProof", opaqueInnerBytes);
    console.log("📜", `  decoded MembershipProof session=${keyOwnerProof.session.toString()} trieNodes.len=${keyOwnerProof.trieNodes.length} validatorCount=${keyOwnerProof.validatorCount.toString()}`);

    // Use the SIGNED variant `grandpa.report_equivocation`. The unsigned
    // variant is restricted to TransactionSource::Local (only block authors
    // via OCW). The signed variant runs the same proof verification path
    // but accepts any signer paying fees.
    const submitter = await pair("//Alice");
    const tx = api.tx.grandpa.reportEquivocation(equivocationProof, keyOwnerProof);
    const info = await tx.paymentInfo(submitter);
    console.log("📜", `  tx weight=${info.weight.toString()} class=${info.class.toString()} len=${tx.encodedLength}`);
    return submitExtrinsic(api, submitter, tx);
}

async function assertBobIsValidator(api, bob) {
    if (!(await sessionContainsValidator(api, bob.address))) {
        throw new Error(`bob is not currently a validator`);
    }
}

async function waitForKicked(api, bob) {
    console.log("📜", `  waiting for bob kick`);
    const period = api.consts.validator.sessionPeriod.toNumber();
    await watchHeads(api, async () => {
        const lock = await api.query.validator.validatorLocks(bob.address);
        if (lock.isSome && lock.unwrap().status.toString() === "Kicked") return true;
    }, period + 1);
}

async function assertEquivocationKickEvent(api, bob) {
    let kickHash = null;
    for (let hash = await api.rpc.chain.getBlockHash(); ;) {
        const apiAt = await api.at(hash);
        const events = await apiAt.query.system.events();
        const hit = events.find(({ event }) =>
            event.section === "validator" &&
            event.method === "ValidatorKicked" &&
            event.data[0].toString() === bob.address &&
            event.data[1].toString() === "Equivocation"
        );
        if (hit) { kickHash = hash; break; }
        const header = await api.rpc.chain.getHeader(hash);
        if (header.number.toNumber() === 0) break;
        hash = header.parentHash;
    }
    if (!kickHash) {
        throw new Error(`no ValidatorKicked{Equivocation} event found for bob`);
    }
    console.log("📜", `  equivocation kick event found at ${kickHash}`);
}

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
    const deadline =  cd.unwrap().toNumber();

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

        await assertBobIsValidator(api, bob);
        await reportBobEquivocation(api);
        await waitForKicked(api, bob);
        await assertEquivocationKickEvent(api, bob);
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
