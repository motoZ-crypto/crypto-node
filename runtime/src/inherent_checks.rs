
/// Maximum allowed timestamp drift from the node's local clock (milliseconds).
pub const MAX_TIMESTAMP_DRIFT_MS: u64 = 2_000;

/// Validate block timestamp against drift limits.
///
/// Appends errors to `result` if the block timestamp exceeds the allowed
/// drift or is earlier than the parent timestamp.
pub fn check_timestamp_drift(
	result: &mut sp_inherents::CheckInherentsResult,
	block_ts_ms: u64,
	node_ts_ms: u64,
	parent_ts_ms: u64,
) {
	if block_ts_ms > node_ts_ms + MAX_TIMESTAMP_DRIFT_MS {
		let _ = result.put_error(
			sp_timestamp::INHERENT_IDENTIFIER,
			&sp_timestamp::InherentError::TooFarInFuture,
		);
	}

	if block_ts_ms < parent_ts_ms {
		let _ = result.put_error(
			sp_timestamp::INHERENT_IDENTIFIER,
			&sp_timestamp::InherentError::TooEarly,
		);
	}
}
