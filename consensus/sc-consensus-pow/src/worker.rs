use futures::{
	prelude::*,
	task::{Context, Poll},
};
use futures_timer::Delay;
use log::*;
use parking_lot::Mutex;
use sc_client_api::ImportNotifications;
use sc_consensus::{BlockImportParams, BoxBlockImport, StateAction, StorageChanges};
use sp_consensus::{BlockOrigin, Proposal};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
	DigestItem,
};
use std::{
	collections::HashMap,
	pin::Pin,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	},
	time::Duration,
};

use crate::{PowAlgorithm, Seal, LOG_TARGET, POW_ENGINE_ID};

/// Mining metadata. This is the information needed to start an actual mining loop.
#[derive(Clone, Eq, PartialEq)]
pub struct MiningMetadata<H, D> {
	/// Currently known best hash which the pre-hash is built on.
	pub best_hash: H,
	/// Mining pre-hash.
	pub pre_hash: H,
	/// Pre-runtime digest item.
	pub pre_runtime: Option<Vec<u8>>,
	/// Mining target difficulty.
	pub difficulty: D,
}

/// A build of mining, containing the metadata and the block proposal.
pub struct MiningBuild<Block: BlockT, Algorithm: PowAlgorithm<Block>> {
	/// Mining metadata.
	pub metadata: MiningMetadata<Block::Hash, Algorithm::Difficulty>,
	/// Mining proposal.
	pub proposal: Proposal<Block>,
}

/// Every mining build kept for the current best chain head.
///
/// A fresh task is generated on each worker tick and stacked here, so a miner
/// may solve any task still tied to the current head, new or old. The head
/// moving on clears the lot.
struct MiningBuilds<Block: BlockT, Algorithm: PowAlgorithm<Block>> {
	/// Head every stored task builds on.
	best_hash: Option<Block::Hash>,
	/// Pre-hash of the most recent task, handed to fresh miners.
	latest: Option<Block::Hash>,
	/// Tasks keyed by pre-hash.
	tasks: HashMap<Block::Hash, MiningBuild<Block, Algorithm>>,
}

impl<Block: BlockT, Algorithm: PowAlgorithm<Block>> Default for MiningBuilds<Block, Algorithm> {
	fn default() -> Self {
		Self { best_hash: None, latest: None, tasks: HashMap::new() }
	}
}

/// Version of the mining worker.
#[derive(Eq, PartialEq, Clone, Copy)]
pub struct Version(usize);

/// Mining worker that exposes structs to query the current mining build and submit mined blocks.
pub struct MiningHandle<
	Block: BlockT,
	Algorithm: PowAlgorithm<Block>,
	L: sc_consensus::JustificationSyncLink<Block>,
> {
	version: Arc<AtomicUsize>,
	algorithm: Arc<Algorithm>,
	justification_sync_link: Arc<L>,
	build: Arc<Mutex<MiningBuilds<Block, Algorithm>>>,
	block_import: Arc<Mutex<BoxBlockImport<Block>>>,
}

impl<Block, Algorithm, L> MiningHandle<Block, Algorithm, L>
where
	Block: BlockT,
	Algorithm: PowAlgorithm<Block>,
	Algorithm::Difficulty: 'static + Send,
	L: sc_consensus::JustificationSyncLink<Block>,
{
	fn increment_version(&self) {
		self.version.fetch_add(1, Ordering::SeqCst);
	}

	pub(crate) fn new(
		algorithm: Algorithm,
		block_import: BoxBlockImport<Block>,
		justification_sync_link: L,
	) -> Self {
		Self {
			version: Arc::new(AtomicUsize::new(0)),
			algorithm: Arc::new(algorithm),
			justification_sync_link: Arc::new(justification_sync_link),
			build: Arc::new(Mutex::new(MiningBuilds::default())),
			block_import: Arc::new(Mutex::new(block_import)),
		}
	}

	pub(crate) fn on_major_syncing(&self) {
		*self.build.lock() = MiningBuilds::default();
		self.increment_version();
	}

	pub(crate) fn on_build(&self, value: MiningBuild<Block, Algorithm>) {
		let best_hash = value.metadata.best_hash;
		let pre_hash = value.metadata.pre_hash;

		let mut builds = self.build.lock();
		if builds.best_hash != Some(best_hash) {
			// The head moved on, so every task built on the old head is dead.
			builds.best_hash = Some(best_hash);
			builds.tasks.clear();
		}
		builds.tasks.insert(pre_hash, value);
		builds.latest = Some(pre_hash);
		drop(builds);

		self.increment_version();
	}

	/// Get the version of the mining worker.
	///
	/// This returns type `Version` which can only compare equality. If `Version` is unchanged, then
	/// it can be certain that `best_hash` and `metadata` were not changed.
	pub fn version(&self) -> Version {
		Version(self.version.load(Ordering::SeqCst))
	}

	/// Get the current best hash. `None` if the worker has just started or the client is doing
	/// major syncing.
	pub fn best_hash(&self) -> Option<Block::Hash> {
		self.build.lock().best_hash
	}

	/// Get a copy of the most recent mining metadata, if available.
	pub fn metadata(&self) -> Option<MiningMetadata<Block::Hash, Algorithm::Difficulty>> {
		let builds = self.build.lock();
		builds.latest.and_then(|pre_hash| builds.tasks.get(&pre_hash)).map(|b| b.metadata.clone())
	}

	/// Submit a seal found for `pre_hash`. The seal is validated again before
	/// import. Returns true on a successful import. A `pre_hash` the head has
	/// already moved past is no longer stored, so it returns false.
	#[allow(clippy::await_holding_lock)]
	pub async fn submit(&self, pre_hash: Block::Hash, seal: Seal) -> bool {
		let metadata = match self.build.lock().tasks.get(&pre_hash) {
			Some(build) => build.metadata.clone(),
			None => {
				warn!(target: LOG_TARGET, "Unable to import mined block: no task for the submitted pre-hash",);
				return false;
			},
		};

		// Pre-check against the same realtime difficulty import recomputes.
		let difficulty = match self.algorithm.difficulty(metadata.best_hash) {
			Ok(difficulty) => difficulty,
			Err(err) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: {}", err,);
				return false;
			},
		};

		match self.algorithm.verify(
			&BlockId::Hash(metadata.best_hash),
			&metadata.pre_hash,
			metadata.pre_runtime.as_ref().map(|v| &v[..]),
			&seal,
			difficulty,
		) {
			Ok(true) => (),
			Ok(false) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: seal is invalid",);
				return false;
			},
			Err(err) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: {}", err,);
				return false;
			},
		}

		let build = match self.build.lock().tasks.remove(&pre_hash) {
			Some(build) => build,
			None => {
				warn!(target: LOG_TARGET, "Unable to import mined block: task already taken",);
				return false;
			},
		};
		self.increment_version();

		let seal = DigestItem::Seal(POW_ENGINE_ID, seal);
		let (header, body) = build.proposal.block.deconstruct();

		let mut import_block = BlockImportParams::new(BlockOrigin::Own, header);
		import_block.post_digests.push(seal);
		import_block.body = Some(body);
		import_block.state_action =
			StateAction::ApplyChanges(StorageChanges::Changes(build.proposal.storage_changes));

		let header = import_block.post_header();
		let block_import = self.block_import.lock();

		match block_import.import_block(import_block).await {
			Ok(res) => {
				res.handle_justification(
					&header.hash(),
					*header.number(),
					&self.justification_sync_link,
				);

				// The block landed; drop every remaining task for the now-stale head.
				*self.build.lock() = MiningBuilds::default();
				self.increment_version();

				info!(
					target: LOG_TARGET,
					"✅ Successfully mined block on top of: {}", build.metadata.best_hash
				);
				true
			},
			Err(err) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: {}", err,);
				false
			},
		}
	}
}

impl<Block, Algorithm, L> Clone for MiningHandle<Block, Algorithm, L>
where
	Block: BlockT,
	Algorithm: PowAlgorithm<Block>,
	L: sc_consensus::JustificationSyncLink<Block>,
{
	fn clone(&self) -> Self {
		Self {
			version: self.version.clone(),
			algorithm: self.algorithm.clone(),
			justification_sync_link: self.justification_sync_link.clone(),
			build: self.build.clone(),
			block_import: self.block_import.clone(),
		}
	}
}

/// A stream that waits for a block import or timeout.
pub struct UntilImportedOrTimeout<Block: BlockT> {
	import_notifications: ImportNotifications<Block>,
	timeout: Duration,
	inner_delay: Option<Delay>,
}

impl<Block: BlockT> UntilImportedOrTimeout<Block> {
	/// Create a new stream using the given import notification and timeout duration.
	pub fn new(import_notifications: ImportNotifications<Block>, timeout: Duration) -> Self {
		Self { import_notifications, timeout, inner_delay: None }
	}
}

impl<Block: BlockT> Stream for UntilImportedOrTimeout<Block> {
	type Item = ();

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<()>> {
		let mut fire = false;

		loop {
			match Stream::poll_next(Pin::new(&mut self.import_notifications), cx) {
				Poll::Pending => break,
				Poll::Ready(Some(_)) => {
					fire = true;
				},
				Poll::Ready(None) => return Poll::Ready(None),
			}
		}

		let timeout = self.timeout;
		let inner_delay = self.inner_delay.get_or_insert_with(|| Delay::new(timeout));

		match Future::poll(Pin::new(inner_delay), cx) {
			Poll::Pending => (),
			Poll::Ready(()) => {
				fire = true;
			},
		}

		if fire {
			self.inner_delay = None;
			Poll::Ready(Some(()))
		} else {
			Poll::Pending
		}
	}
}
