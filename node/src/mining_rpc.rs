//! External mining RPC.
//!
//! Lets miners running outside the node pull the current mining task and submit
//! a seal they found. The node revalidates every submission through the regular
//! import path, so external compute is never trusted. The seal carries only the
//! nonce and work; the reward address is pinned in the pre-runtime digest the
//! node builds, so an external miner cannot redirect the reward.

use std::sync::Arc;

use jsonrpsee::{
	core::{async_trait, RpcResult},
	proc_macros::rpc,
	types::ErrorObjectOwned,
};
use serde::{Deserialize, Serialize};
use sp_core::{Bytes, H256, U256};

/// No mining build exists yet (worker just started or syncing).
const NO_TASK: i32 = 9001;
/// Submission targets a pre-hash the best chain head has already moved past.
const STALE_TASK: i32 = 9002;

/// The mining worker as the RPC sees it, reading the live task and handing a seal
/// back for import. Keeps the consensus generics out of the RPC layer.
///
/// `submit_seal` blocks on the full import, so callers must run it off the async
/// executor.
pub trait ExternalMiner: Send + Sync {
	/// Pre-hash and difficulty of the current build, or `None` before the first
	/// build lands.
	fn current_task(&self) -> Option<(H256, U256)>;

	/// Revalidate and import a raw seal. Returns `true` when the block imports.
	fn submit_seal(&self, seal: Vec<u8>) -> bool;
}

/// Everything an external miner needs to compute the work for one task.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiningTask {
	/// Block pre-hash the seal must be mined against.
	pub pre_hash: H256,
	/// Difficulty target the resulting work must satisfy.
	pub difficulty: U256,
	/// Reward and seed-bound miner address, in SS58.
	pub miner: String,
	/// Protocol string pinning the algorithm, resolution and quantization.
	pub protocol: String,
}

/// RPC surface for external miners.
#[rpc(server)]
pub trait MiningApi {
	/// Pull the current mining task.
	#[method(name = "mining_getTask")]
	fn get_task(&self) -> RpcResult<MiningTask>;

	/// Submit a seal found for `pre_hash`. Rejected when `pre_hash` no longer
	/// matches the current task.
	#[method(name = "mining_submitSeal")]
	async fn submit_seal(&self, pre_hash: H256, seal: Bytes) -> RpcResult<bool>;
}

/// Mining RPC backed by the node's PoW worker.
pub struct Mining {
	handle: Arc<dyn ExternalMiner>,
	miner: String,
	protocol: String,
}

impl Mining {
	/// Build the RPC over a worker handle, the configured reward address and the
	/// active protocol string.
	pub fn new(handle: Arc<dyn ExternalMiner>, miner: String, protocol: String) -> Self {
		Self { handle, miner, protocol }
	}
}

#[async_trait]
impl MiningApiServer for Mining {
	fn get_task(&self) -> RpcResult<MiningTask> {
		let (pre_hash, difficulty) = self.handle.current_task().ok_or_else(no_task)?;
		Ok(MiningTask {
			pre_hash,
			difficulty,
			miner: self.miner.clone(),
			protocol: self.protocol.clone(),
		})
	}

	async fn submit_seal(&self, pre_hash: H256, seal: Bytes) -> RpcResult<bool> {
		let (current, _) = self.handle.current_task().ok_or_else(no_task)?;
		if pre_hash != current {
			return Err(ErrorObjectOwned::owned(
				STALE_TASK,
				"stale pre_hash, the best chain head moved on",
				None::<()>,
			));
		}
		Ok(self.handle.submit_seal(seal.0))
	}
}

fn no_task() -> ErrorObjectOwned {
	ErrorObjectOwned::owned(NO_TASK, "no mining task available yet", None::<()>)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	/// Records submitted seals and returns a fixed task and import verdict.
	struct MockMiner {
		task: Option<(H256, U256)>,
		imported: bool,
		submitted: Mutex<Vec<Vec<u8>>>,
	}

	impl MockMiner {
		fn new(task: Option<(H256, U256)>, imported: bool) -> Arc<Self> {
			Arc::new(Self { task, imported, submitted: Mutex::new(Vec::new()) })
		}
	}

	impl ExternalMiner for MockMiner {
		fn current_task(&self) -> Option<(H256, U256)> {
			self.task
		}

		fn submit_seal(&self, seal: Vec<u8>) -> bool {
			self.submitted.lock().unwrap().push(seal);
			self.imported
		}
	}

	fn mining(mock: Arc<MockMiner>) -> Mining {
		Mining::new(mock, "5Miner".into(), "proto-v1".into())
	}

	#[test]
	fn get_task_exposes_full_inputs() {
		let pre_hash = H256::from_low_u64_be(1);
		let mock = MockMiner::new(Some((pre_hash, U256::from(7))), true);
		let task = mining(mock).get_task().expect("task is available");
		assert_eq!(task.pre_hash, pre_hash);
		assert_eq!(task.difficulty, U256::from(7));
		assert_eq!(task.miner, "5Miner");
		assert_eq!(task.protocol, "proto-v1");
	}

	#[test]
	fn get_task_without_build_errors() {
		let err = mining(MockMiner::new(None, true)).get_task().unwrap_err();
		assert_eq!(err.code(), NO_TASK);
	}

	#[test]
	fn submit_seal_imports_for_current_pre_hash() {
		let pre_hash = H256::from_low_u64_be(2);
		let mock = MockMiner::new(Some((pre_hash, U256::from(1))), true);
		let imported = futures::executor::block_on(
			mining(mock.clone()).submit_seal(pre_hash, Bytes(vec![1, 2, 3])),
		)
		.expect("submission runs");
		assert!(imported);
		assert_eq!(mock.submitted.lock().unwrap().as_slice(), &[vec![1, 2, 3]]);
	}

	#[test]
	fn submit_seal_rejects_stale_pre_hash() {
		let mock = MockMiner::new(Some((H256::from_low_u64_be(2), U256::from(1))), true);
		let err = futures::executor::block_on(
			mining(mock.clone()).submit_seal(H256::from_low_u64_be(9), Bytes(vec![1])),
		)
		.unwrap_err();
		assert_eq!(err.code(), STALE_TASK);
		assert!(mock.submitted.lock().unwrap().is_empty(), "stale seal must not reach import");
	}

	#[test]
	fn submit_seal_without_build_errors() {
		let err = futures::executor::block_on(
			mining(MockMiner::new(None, true)).submit_seal(H256::zero(), Bytes(vec![1])),
		)
		.unwrap_err();
		assert_eq!(err.code(), NO_TASK);
	}
}
