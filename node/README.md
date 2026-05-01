## Some notes

### GRANDPA

GRANDPA is wired in unconditionally.

Upstream gates the entire GRANDPA stack behind an `enable_grandpa` flag (`!cli.no_grandpa && role.is_authority()`). When the flag is false, upstream:

* never calls `sc_consensus_grandpa::block_import()` (so no notification stream is created),
* never registers the `/grandpa/...` gossip protocol on the network,
* never spawns `run_grandpa_voter`.

We cannot do that here for two reasons:

* **GRANDPA is wrapped by PoW import.**
  `PowBlockImport::new(grandpa_block_import.clone(), …)` requires a `grandpa_block_import` to exist on every node so that finality justifications attached to incoming PoW blocks are verified consistently.
  That call has the side effect of registering the GRANDPA gossip protocol and creating the notification service.
* **The notification service must be drained.**
  Once the gossip protocol is registered, the network task pushes messages into it.
  If the receiver end is dropped (the original behaviour for non-authorities) the next push triggers `EssentialTaskClosed` and the node exits.

We therefore **always** spawn `sc_consensus_grandpa::run_grandpa_voter`. Authority nodes pass `keystore = Some(...)` and act as full voters; non-authority nodes pass `keystore = None` and act as observers (follow finality, never vote, never produce equivocation reports). The task name is set accordingly (`grandpa-voter` vs `grandpa-observer`) so the difference is visible in logs and prometheus metrics.

This is intentional: the multi-node integration tests rely on non-validator nodes (`dave`, `eve`) reporting the same finalized hash as the validators, and downstream RPC consumers expect a meaningful `finalized_head` from any full node.
