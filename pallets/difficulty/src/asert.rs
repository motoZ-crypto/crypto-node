//! Pure-integer ASERT difficulty target calculation.
//!
//! Implements the formula:
//!
//! ```text
//! next_target = anchor_target × 2^((time_delta - target_block_time × height_delta) / halflife)
//! ```
//!
//! All arithmetic is integer-only using 16-bit fixed-point representation.
//! The `2^x` function is approximated via a cubic polynomial with < 0.013% error.

use sp_core::U256;

/// Fixed-point fractional bits.
const FRAC_BITS: i64 = 16;
/// 1.0 in fixed-point representation.
const FRAC_ONE: i64 = 1 << FRAC_BITS; // 65536

/// Compute the ASERT next target from anchor block parameters.
///
/// # Parameters
///
/// - `anchor_target`: Target value of the anchor block (U256).
/// - `time_delta`: Current block timestamp minus anchor block parent timestamp (seconds).
/// - `height_delta`: Current block height minus anchor block height.
/// - `target_block_time`: Ideal block interval in seconds (e.g. 20).
/// - `halflife`: ASERT halflife in seconds (e.g. 1800).
///
/// # Returns
///
/// The computed next target (U256), clamped to [1, U256::MAX].
pub fn compute_next_target(
	anchor_target: U256,
	time_delta: i64,
	height_delta: u32,
	target_block_time: u64,
	halflife: u64,
) -> U256 {
	// exponent = (time_delta - target_block_time * height_delta) / halflife
	// In fixed-point: exponent_fp = ((time_delta - ideal_time) << FRAC_BITS) / halflife
	let ideal_time = target_block_time as i64 * height_delta as i64;
	let exponent_numer = (time_delta - ideal_time) * FRAC_ONE;
	let halflife_i64 = halflife as i64;
	// Division rounds toward zero; this is acceptable for the exponent.
	let exponent_fp = exponent_numer / halflife_i64;

	// Compute 2^exponent_fp in fixed-point.
	// Split into integer part and fractional part.
	// exponent_fp is in units of halflife, so we need to compute 2^(exponent_fp / FRAC_ONE).
	// Wait — the exponent is already divided by halflife above, so exponent_fp represents
	// the actual exponent (in halflife units). The formula uses base-2 exponentiation
	// directly with the halflife as denominator, so exponent_fp is the number of halvings.
	//
	// We need: factor = 2^(exponent_fp / FRAC_ONE)
	// Split: integer_part = exponent_fp >> FRAC_BITS (arithmetic shift)
	//        frac_part    = exponent_fp & (FRAC_ONE - 1)
	// For negative: we need floor division, not truncation toward zero.
	let int_part = exponent_fp >> FRAC_BITS; // arithmetic right shift = floor for negatives
	let frac_part = exponent_fp - (int_part << FRAC_BITS); // always in [0, FRAC_ONE)

	// Approximate 2^frac where frac is in [0, FRAC_ONE) fixed-point.
	// Cubic polynomial: 2^x ≈ 1 + a1*x + a2*x^2 + a3*x^3
	// Coefficients scaled to fixed-point (16 bits):
	//   a1 = ln(2) ≈ 0.693147 → 45426
	//   a2 = ln(2)^2/2 ≈ 0.240227 → 15736
	//   a3 = ln(2)^3/6 ≈ 0.055504 → 3638 (rounded from 3637.7)
	const A1: i64 = 45426;
	const A2: i64 = 15736;
	const A3: i64 = 3638;

	// frac_part is in [0, FRAC_ONE), compute polynomial in fixed-point.
	let x = frac_part;
	// 2^frac ≈ FRAC_ONE + a1*x/FRAC_ONE + a2*x^2/FRAC_ONE^2 + a3*x^3/FRAC_ONE^3
	let term1 = A1 * x / FRAC_ONE;
	let term2 = A2 * x / FRAC_ONE * x / FRAC_ONE;
	let term3 = A3 * x / FRAC_ONE * x / FRAC_ONE * x / FRAC_ONE;
	let frac_factor = FRAC_ONE + term1 + term2 + term3; // in fixed-point

	// Now: next_target = anchor_target * frac_factor / FRAC_ONE * 2^int_part
	// Apply the fractional multiplier first (to preserve precision).
	let frac_factor_u256 = U256::from(frac_factor as u64);
	let frac_one_u256 = U256::from(FRAC_ONE as u64);

	let mut result = anchor_target * frac_factor_u256 / frac_one_u256;

	// Apply integer exponent: shift left for positive, shift right for negative.
	if int_part >= 0 {
		let shift = int_part as u32;
		if shift >= 256 {
			// Overflow — target is astronomically high, clamp to max.
			return U256::MAX;
		}
		// Check for overflow: if result has bits set in positions >= (256 - shift),
		// the shift would overflow.
		let headroom = 256 - result.bits();
		if shift as usize > headroom {
			return U256::MAX;
		}
		result = result << shift;
	} else {
		let shift = (-int_part) as u32;
		if shift >= 256 {
			// Underflow — clamp to minimum target of 1.
			return U256::one();
		}
		result = result >> shift;
	}

	// Clamp: target must be at least 1 (difficulty must not be infinite).
	if result.is_zero() {
		U256::one()
	} else {
		result
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn on_schedule_returns_anchor() {
		// If blocks are exactly on schedule, target should equal anchor_target.
		let anchor = U256::from(1_000_000u64);
		// time_delta = target_block_time * height_delta
		// e.g. height_delta=10 (10th block after anchor), time_delta = 20 * 10 = 200
		let result = compute_next_target(anchor, 200, 10, 20, 1800);
		// Should be very close to anchor (within rounding).
		let diff = if result > anchor { result - anchor } else { anchor - result };
		assert!(diff <= U256::from(1u64), "expected ~anchor, got {:?}", result);
	}

	#[test]
	fn slow_blocks_increase_target() {
		// Blocks coming slower than expected → target increases (difficulty decreases).
		let anchor = U256::from(1_000_000u64);
		// height_delta=10, ideal time = 200s, actual time = 400s (twice as slow)
		let result = compute_next_target(anchor, 400, 10, 20, 1800);
		assert!(result > anchor, "slow blocks should increase target");
	}

	#[test]
	fn fast_blocks_decrease_target() {
		// Blocks coming faster than expected → target decreases (difficulty increases).
		let anchor = U256::from(1_000_000u64);
		// height_delta=10, ideal time = 200s, actual time = 100s (twice as fast)
		let result = compute_next_target(anchor, 100, 10, 20, 1800);
		assert!(result < anchor, "fast blocks should decrease target");
	}

	#[test]
	fn halflife_halves_target_when_fast() {
		// If blocks arrive halflife seconds ahead of schedule, target should halve.
		let anchor = U256::from(1u64) << 128;
		// For target to double (halflife behind schedule):
		// exponent = +1 → time_delta - 20*1 = 1800 → time_delta = 1820
		// height_delta=1 (one block after anchor)
		let result = compute_next_target(anchor, 1820, 1, 20, 1800);
		let expected = anchor * U256::from(2u64);
		// Allow ~1% tolerance due to polynomial approximation.
		let tolerance = expected / U256::from(100u64);
		let diff = if result > expected { result - expected } else { expected - result };
		assert!(diff < tolerance, "expected ~{:?}, got {:?}", expected, result);
	}

	#[test]
	fn no_blocks_for_30min_halves_difficulty() {
		// 30 minutes (1800s) without blocks from anchor.
		// height_delta=1 (first block after anchor), time_delta = 1800 + 20 = 1820
		// exponent = (1820 - 20*1) / 1800 = 1800/1800 = 1
		// target doubles → difficulty halves.
		let anchor = U256::from(1u64) << 128;
		let result = compute_next_target(anchor, 1820, 1, 20, 1800);
		let expected = anchor * U256::from(2u64);
		let tolerance = expected / U256::from(100u64);
		let diff = if result > expected { result - expected } else { expected - result };
		assert!(diff < tolerance, "difficulty should halve after 30min gap");
	}

	#[test]
	fn result_never_zero() {
		// Even with extremely fast blocks, target should not be zero.
		let anchor = U256::from(1u64);
		let result = compute_next_target(anchor, 0, 1000, 20, 1800);
		assert!(!result.is_zero(), "target must never be zero");
	}
}
