use sc_service::ChainType;
use serde_json::json;
use solochain_template_runtime::{
	genesis_config_presets::INTEGRATION_RUNTIME_PRESET, WASM_BINARY,
};

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

fn chain_properties() -> sc_service::Properties {
	serde_json::from_value(json!({
		"tokenDecimals": 18,
		"tokenSymbol": "UNIT"
	}))
	.expect("valid properties")
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Development")
	.with_id("dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}

pub fn integration_chain_spec() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Integration Testnet")
	.with_id("integration")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(INTEGRATION_RUNTIME_PRESET)
	.with_properties(chain_properties())
	.build())
}
