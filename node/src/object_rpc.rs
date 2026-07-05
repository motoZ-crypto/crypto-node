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

	#[test]
	fn obj_shifts_faces_to_one_based() {
		let mesh = WireMesh {
			vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
			faces: vec![[0, 1, 2]],
		};
		assert_eq!(write_obj(&mesh), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
	}
}
