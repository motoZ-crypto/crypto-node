//! Service and ServiceFactory implementation. Specialized wrapper over substrate service.

use futures::FutureExt;
use sc_client_api::Backend;
use sc_consensus::LongestChain;
use sc_consensus_pow::PowBlockImport;
use sc_service::{error::Error as ServiceError, Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sc_transaction_pool_api::OffchainTransactionPoolFactory;
use sha256pow::Sha256DoubleHashAlgorithm;
use solochain_template_runtime::{self, apis::RuntimeApi, opaque::Block};
use sp_core::U256;
use sp_keyring::Sr25519Keyring;
use std::{sync::Arc, time::Duration};

pub(crate) type FullClient = sc_service::TFullClient<
	Block,
	RuntimeApi,
	sc_executor::WasmExecutor<sp_io::SubstrateHostFunctions>,
>;
type FullBackend = sc_service::TFullBackend<Block>;
type FullSelectChain = LongestChain<FullBackend, Block>;

pub type Service = sc_service::PartialComponents<
	FullClient,
	FullBackend,
	FullSelectChain,
	sc_consensus::DefaultImportQueue<Block>,
	sc_transaction_pool::TransactionPoolHandle<Block, FullClient>,
	Option<Telemetry>,
>;

pub fn new_partial(config: &Configuration) -> Result<Service, ServiceError> {
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	let executor = sc_service::new_wasm_executor::<sp_io::SubstrateHostFunctions>(&config.executor);
	let (client, backend, keystore_container, task_manager) =
		sc_service::new_full_parts::<Block, RuntimeApi, _>(
			config,
			telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
			executor,
			vec![],
		)?;
	let client = Arc::new(client);

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", None, worker.run());
		telemetry
	});

	let select_chain = LongestChain::new(backend.clone());

	let transaction_pool = Arc::from(
		sc_transaction_pool::Builder::new(
			task_manager.spawn_essential_handle(),
			client.clone(),
			config.role.is_authority().into(),
		)
		.with_options(config.transaction_pool.clone())
		.with_prometheus(config.prometheus_registry())
		.build(),
	);

	let algorithm = Sha256DoubleHashAlgorithm::new(client.clone());

	let pow_block_import = PowBlockImport::new(
		client.clone(),
		client.clone(),
		algorithm.clone(),
		0u32.into(),
		select_chain.clone(),
		move |_, ()| async { Ok(sp_timestamp::InherentDataProvider::from_system_time()) },
	);

	let import_queue = sc_consensus_pow::import_queue(
		Box::new(pow_block_import),
		None,
		algorithm,
		&task_manager.spawn_essential_handle(),
		config.prometheus_registry(),
	)?;

	Ok(sc_service::PartialComponents {
		client,
		backend,
		task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: telemetry,
	})
}

/// Builds a new service for a full client.
pub fn new_full<
	N: sc_network::NetworkBackend<Block, <Block as sp_runtime::traits::Block>::Hash>,
>(
	config: Configuration,
	mine: bool,
) -> Result<TaskManager, ServiceError> {
	let sc_service::PartialComponents {
		client,
		backend,
		mut task_manager,
		import_queue,
		keystore_container,
		select_chain,
		transaction_pool,
		other: mut telemetry,
	} = new_partial(&config)?;

	let net_config = sc_network::config::FullNetworkConfiguration::<
		Block,
		<Block as sp_runtime::traits::Block>::Hash,
		N,
	>::new(&config.network, config.prometheus_registry().cloned());
	let metrics = N::register_notification_metrics(config.prometheus_registry());

	let (network, system_rpc_tx, tx_handler_controller, sync_service) =
		sc_service::build_network(sc_service::BuildNetworkParams {
			config: &config,
			net_config,
			client: client.clone(),
			transaction_pool: transaction_pool.clone(),
			spawn_handle: task_manager.spawn_handle(),
			spawn_essential_handle: task_manager.spawn_essential_handle(),
			import_queue,
			block_announce_validator_builder: None,
			warp_sync_config: None,
			block_relay: None,
			metrics,
		})?;

	if config.offchain_worker.enabled {
		let offchain_workers =
			sc_offchain::OffchainWorkers::new(sc_offchain::OffchainWorkerOptions {
				runtime_api_provider: client.clone(),
				is_validator: config.role.is_authority(),
				keystore: Some(keystore_container.keystore()),
				offchain_db: backend.offchain_storage(),
				transaction_pool: Some(OffchainTransactionPoolFactory::new(
					transaction_pool.clone(),
				)),
				network_provider: Arc::new(network.clone()),
				enable_http_requests: true,
				custom_extensions: |_| vec![],
			})?;
		task_manager.spawn_handle().spawn(
			"offchain-workers-runner",
			"offchain-worker",
			offchain_workers.run(client.clone(), task_manager.spawn_handle()).boxed(),
		);
	}

	let prometheus_registry = config.prometheus_registry().cloned();

	let rpc_extensions_builder = {
		let client = client.clone();
		let pool = transaction_pool.clone();

		Box::new(move |_| {
			let deps = crate::rpc::FullDeps { client: client.clone(), pool: pool.clone() };
			crate::rpc::create_full(deps).map_err(Into::into)
		})
	};

	let _rpc_handlers = sc_service::spawn_tasks(sc_service::SpawnTasksParams {
		network: Arc::new(network.clone()),
		client: client.clone(),
		keystore: keystore_container.keystore(),
		task_manager: &mut task_manager,
		transaction_pool: transaction_pool.clone(),
		rpc_builder: rpc_extensions_builder,
		backend,
		system_rpc_tx,
		tx_handler_controller,
		sync_service: sync_service.clone(),
		config,
		telemetry: telemetry.as_mut(),
		tracing_execute_block: None,
	})?;

	if mine {
		let proposer_factory = sc_basic_authorship::ProposerFactory::new(
			task_manager.spawn_handle(),
			client.clone(),
			transaction_pool.clone(),
			prometheus_registry.as_ref(),
			telemetry.as_ref().map(|x| x.handle()),
		);

		let algorithm = Sha256DoubleHashAlgorithm::new(client.clone());

		let pow_block_import = PowBlockImport::new(
			client.clone(),
			client.clone(),
			algorithm.clone(),
			0u32.into(),
			select_chain.clone(),
			move |_, ()| async { Ok(sp_timestamp::InherentDataProvider::from_system_time()) },
		);

		// Default miner address: Alice (to be replaced by --miner-address CLI flag).
		let miner_address = Sr25519Keyring::Alice.to_account_id();
		let pre_runtime = codec::Encode::encode(&miner_address);

		let (mining_handle, mining_worker) =
			sc_consensus_pow::start_mining_worker(
				Box::new(pow_block_import),
				client,
				select_chain,
				algorithm,
				proposer_factory,
				sync_service.clone(),
				sync_service,
				Some(pre_runtime),
				move |_, ()| async { Ok(sp_timestamp::InherentDataProvider::from_system_time()) },
				Duration::from_secs(5),
				Duration::from_secs(2),
			);

		task_manager.spawn_essential_handle().spawn_blocking(
			"pow-mining-worker",
			Some("block-authoring"),
			mining_worker,
		);

		std::thread::spawn(move || {
			loop {
				let metadata = match mining_handle.metadata() {
					Some(m) => m,
					None => {
						std::thread::sleep(Duration::from_millis(100));
						continue;
					}
				};

				let pre_hash = metadata.pre_hash;
				let difficulty = metadata.difficulty;
				let best_hash = metadata.best_hash;

				let mut nonce = U256::zero();
				loop {
					let compute = sha256pow::Compute { pre_hash, nonce };
					let work = compute.work();

					if sha256pow::hash_meets_difficulty(&work, difficulty) {
						let seal = compute.seal(difficulty);
						let encoded_seal = codec::Encode::encode(&seal);
						futures::executor::block_on(mining_handle.submit(encoded_seal));
						break;
					}

					nonce = nonce.saturating_add(U256::one());

					if nonce % 10_000 == U256::zero() {
						if let Some(new_meta) = mining_handle.metadata() {
							if new_meta.best_hash != best_hash {
								break;
							}
						}
					}
				}
			}
		});
	}

	Ok(task_manager)
}
