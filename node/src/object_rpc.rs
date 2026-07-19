//! Miner model export RPC.
//!
//! Rebuilds the obj model a block's miner produced. The block records only the
//! nonce, so geometry is regrown from the pre-hash and nonce. To stay faithful
//! across a generator upgrade, the mesh is replayed through the runtime at the
//! block's parent, the same version that verified the block. The node only
//! strips the block apart and formats OBJ.

use std::{fmt::Write, marker::PhantomData, sync::Arc};

use codec::Decode;
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::ErrorObjectOwned};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_consensus_pow::POW_ENGINE_ID;
use sp_core::H256;
use sp_runtime::{
	generic::DigestItem,
	traits::{Block as BlockT, Header as HeaderT},
};

use poscan::{Seal, WireMesh};
use poscan_pow::PowVerifyApi;

/// The requested block is not in the local backend.
const UNKNOWN_BLOCK: i32 = 9101;

/// The block carries no PoW seal, so it holds no miner model.
const NO_SEAL: i32 = 9102;

/// A runtime call or decode step failed.
const REBUILD_FAILED: i32 = 9103;

/// RPC surface for miner model export.
#[rpc(server)]
pub trait ObjectApi {
	/// Rebuild the miner model for `block_hash` and return it as OBJ text.
	#[method(name = "poscan_getObject")]
	fn get_object(&self, block_hash: H256) -> RpcResult<String>;
}

/// Model export backed by the node client. Any node serves it; faithfully
/// exporting an old block needs that block's state retained.
pub struct ObjectExport<C, B> {
	client: Arc<C>,
	_phantom: PhantomData<B>,
}

impl<C, B> ObjectExport<C, B> {
	/// Build the export RPC over a client handle.
	pub fn new(client: Arc<C>) -> Self {
		Self { client, _phantom: PhantomData }
	}
}

impl<C, B> ObjectApiServer for ObjectExport<C, B>
where
	B: BlockT<Hash = H256>,
	C: ProvideRuntimeApi<B> + HeaderBackend<B> + Send + Sync + 'static,
	C::Api: PowVerifyApi<B>,
{
	fn get_object(&self, block_hash: H256) -> RpcResult<String> {
		let mut header = self
			.client
			.header(block_hash)
			.map_err(|e| rpc_err(REBUILD_FAILED, format!("header lookup failed: {e}")))?
			.ok_or_else(|| rpc_err(UNKNOWN_BLOCK, "unknown block".into()))?;

		// The PoW seal is the last digest. Strip it to recover the pre-hash the
		// miner sealed against, exactly as block import does.
		let seal_bytes = match header.digest_mut().pop() {
			Some(DigestItem::Seal(id, bytes)) if id == POW_ENGINE_ID => bytes,
			_ => return Err(rpc_err(NO_SEAL, "block carries no PoW seal".into())),
		};
		let pre_hash = header.hash();
		let parent_hash = *header.parent_hash();

		let seal = Seal::decode(&mut &seal_bytes[..])
			.map_err(|e| rpc_err(REBUILD_FAILED, format!("seal decode failed: {e}")))?;

		// The generator lives in the parent's runtime, so replaying there rebuilds
		// the mesh with the version that sealed the block.
		let encoded = self
			.client
			.runtime_api()
			.generate_mesh(parent_hash, pre_hash, seal.nonce)
			.map_err(|e| rpc_err(REBUILD_FAILED, format!("runtime generate_mesh failed: {e}")))?;
		let mesh = WireMesh::decode(&mut &encoded[..])
			.map_err(|e| rpc_err(REBUILD_FAILED, format!("mesh decode failed: {e}")))?;

		Ok(write_obj(&mesh))
	}
}

/// Format a mesh as Wavefront OBJ text. OBJ counts face indices from one, so
/// each mesh index shifts up by one.
fn write_obj(mesh: &WireMesh) -> String {
	let mut out = String::new();
	for v in &mesh.vertices {
		let _ = writeln!(out, "v {} {} {}", v[0], v[1], v[2]);
	}
	for f in &mesh.faces {
		let _ = writeln!(out, "f {} {} {}", f[0] + 1, f[1] + 1, f[2] + 1);
	}
	out
}

fn rpc_err(code: i32, msg: String) -> ErrorObjectOwned {
	ErrorObjectOwned::owned(code, msg, None::<()>)
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_api::{ApiError, ApiRef, ProvideRuntimeApi};
	use sp_blockchain::{BlockStatus, Info};
	use sp_core::U256;
	use sp_runtime::{
		testing::{Block as RawBlock, Header},
		traits::NumberFor,
		OpaqueExtrinsic,
	};
	use std::{collections::HashMap, sync::Mutex};

	type Block = RawBlock<OpaqueExtrinsic>;

	/// Arguments of every `generate_mesh` replay, in call order.
	type Replays = Arc<Mutex<Vec<(H256, H256, U256)>>>;

	struct MockApi {
		mesh: Vec<u8>,
		replays: Replays,
	}

	sp_api::mock_impl_runtime_apis! {
		impl PowVerifyApi<Block> for MockApi {
			fn verify_seal(&self, _pre_hash: H256, _seal: Vec<u8>, _difficulty: U256) -> bool {
				true
			}

			#[advanced]
			fn generate_mesh(
				&self,
				at: H256,
				pre_hash: H256,
				nonce: U256,
			) -> Result<Vec<u8>, ApiError> {
				self.replays.lock().unwrap().push((at, pre_hash, nonce));
				Ok(self.mesh.clone())
			}
		}
	}

	struct MockClient {
		headers: HashMap<H256, Header>,
		mesh: Vec<u8>,
		replays: Replays,
	}

	impl ProvideRuntimeApi<Block> for MockClient {
		type Api = MockApi;

		fn runtime_api(&self) -> ApiRef<'_, Self::Api> {
			MockApi { mesh: self.mesh.clone(), replays: self.replays.clone() }.into()
		}
	}

	impl HeaderBackend<Block> for MockClient {
		fn header(&self, hash: H256) -> sp_blockchain::Result<Option<Header>> {
			Ok(self.headers.get(&hash).cloned())
		}

		fn info(&self) -> Info<Block> {
			unimplemented!("not used by the export RPC")
		}

		fn status(&self, _hash: H256) -> sp_blockchain::Result<BlockStatus> {
			unimplemented!("not used by the export RPC")
		}

		fn number(&self, _hash: H256) -> sp_blockchain::Result<Option<NumberFor<Block>>> {
			unimplemented!("not used by the export RPC")
		}

		fn hash(&self, _number: NumberFor<Block>) -> sp_blockchain::Result<Option<H256>> {
			unimplemented!("not used by the export RPC")
		}
	}

	fn triangle() -> WireMesh {
		WireMesh {
			vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
			faces: vec![[0, 1, 2]],
		}
	}

	/// A header carrying an author pre-runtime digest, the way mined blocks do.
	/// The pre-hash is taken before the seal lands, exactly like block import.
	fn mined_header(seal_bytes: Vec<u8>) -> (Header, H256, H256) {
		let mut header = Header::new_from_number(5);
		header.parent_hash = H256::repeat_byte(0x11);
		header.digest.push(DigestItem::PreRuntime(POW_ENGINE_ID, vec![0xAB; 32]));
		let pre_hash = header.hash();
		header.digest.push(DigestItem::Seal(POW_ENGINE_ID, seal_bytes));
		let sealed_hash = header.hash();
		(header, sealed_hash, pre_hash)
	}

	fn export(headers: Vec<(H256, Header)>) -> (ObjectExport<MockClient, Block>, Replays) {
		let replays = Replays::default();
		let client = MockClient {
			headers: headers.into_iter().collect(),
			mesh: triangle().encode(),
			replays: replays.clone(),
		};
		(ObjectExport::new(Arc::new(client)), replays)
	}

	#[test]
	fn obj_shifts_faces_to_one_based() {
		let mesh = triangle();
		assert_eq!(write_obj(&mesh), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
	}

	#[test]
	fn get_object_takes_every_replay_input_from_the_block() {
		let nonce = U256::from(42);
		let seal = Seal { nonce, work: H256::repeat_byte(0x09) };
		let (header, sealed_hash, pre_hash) = mined_header(seal.encode());
		let parent_hash = header.parent_hash;
		let (export, replays) = export(vec![(sealed_hash, header)]);

		let obj = export.get_object(sealed_hash).expect("export succeeds");

		assert_eq!(obj, write_obj(&triangle()));
		assert_eq!(
			replays.lock().unwrap().as_slice(),
			&[(parent_hash, pre_hash, nonce)],
			"the mesh replays at the parent, on the seal-stripped hash, with the seal nonce",
		);
	}

	#[test]
	fn get_object_rejects_block_without_pow_seal() {
		let mut header = Header::new_from_number(5);
		header.digest.push(DigestItem::PreRuntime(POW_ENGINE_ID, vec![0xAB; 32]));
		let hash = header.hash();
		let (export, replays) = export(vec![(hash, header)]);

		let err = export.get_object(hash).expect_err("sealless block must be rejected");

		assert_eq!(err.code(), NO_SEAL);
		assert!(replays.lock().unwrap().is_empty());
	}

	#[test]
	fn get_object_rejects_malformed_seal_bytes() {
		let (header, sealed_hash, _) = mined_header(vec![0xFF]);
		let (export, replays) = export(vec![(sealed_hash, header)]);

		let err = export.get_object(sealed_hash).expect_err("undecodable seal must be rejected");

		assert_eq!(err.code(), REBUILD_FAILED);
		assert!(err.message().contains("seal decode"), "unexpected message: {}", err.message());
		assert!(replays.lock().unwrap().is_empty());
	}

	#[test]
	fn get_object_rejects_unknown_block() {
		let (export, _) = export(Vec::new());

		let err = export
			.get_object(H256::repeat_byte(0x77))
			.expect_err("a hash outside the backend must be rejected");

		assert_eq!(err.code(), UNKNOWN_BLOCK);
	}
}
