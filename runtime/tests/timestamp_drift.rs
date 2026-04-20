use solochain_template_runtime::apis::{check_timestamp_drift, MAX_TIMESTAMP_DRIFT_MS};
use sp_inherents::CheckInherentsResult;

fn empty_result() -> CheckInherentsResult {
	CheckInherentsResult::new()
}

#[test]
fn max_drift_constant_is_2s() {
	assert_eq!(MAX_TIMESTAMP_DRIFT_MS, 2_000);
}

#[test]
fn within_drift_accepted() {
	let mut result = empty_result();
	// node=100s, block=101s (1s drift), parent=90s
	check_timestamp_drift(&mut result, 101_000, 100_000, 90_000);
	assert!(result.ok(), "1s drift should be accepted");
}

#[test]
fn exact_drift_boundary_accepted() {
	let mut result = empty_result();
	// node=100s, block=102s (exactly 2s drift), parent=90s
	check_timestamp_drift(&mut result, 102_000, 100_000, 90_000);
	assert!(result.ok(), "exactly 2s drift should be accepted");
}

#[test]
fn just_over_drift_boundary_rejected() {
	let mut result = empty_result();
	// node=100s, block=102.001s (2.001s drift), parent=90s
	check_timestamp_drift(&mut result, 102_001, 100_000, 90_000);
	assert!(!result.ok(), "2.001s drift should be rejected");
}

#[test]
fn large_future_drift_rejected() {
	let mut result = empty_result();
	// node=100s, block=110s (10s drift), parent=90s
	check_timestamp_drift(&mut result, 110_000, 100_000, 90_000);
	assert!(!result.ok(), "10s drift should be rejected");
}

#[test]
fn before_parent_rejected() {
	let mut result = empty_result();
	// node=100s, block=89s, parent=90s
	check_timestamp_drift(&mut result, 89_000, 100_000, 90_000);
	assert!(!result.ok(), "timestamp before parent should be rejected");
}

#[test]
fn equal_to_parent_accepted() {
	let mut result = empty_result();
	// node=100s, block=90s, parent=90s
	check_timestamp_drift(&mut result, 90_000, 100_000, 90_000);
	assert!(result.ok(), "timestamp equal to parent should be accepted");
}

#[test]
fn after_parent_within_drift_accepted() {
	let mut result = empty_result();
	// node=100s, block=95s, parent=90s
	check_timestamp_drift(&mut result, 95_000, 100_000, 90_000);
	assert!(result.ok(), "block between parent and node time should be accepted");
}

#[test]
fn zero_drift_accepted() {
	let mut result = empty_result();
	// node=100s, block=100s, parent=90s
	check_timestamp_drift(&mut result, 100_000, 100_000, 90_000);
	assert!(result.ok(), "zero drift should be accepted");
}
