#!/usr/bin/env bash
# Run every zombienet scenario (auto-discovered under scenarios/) with a
# controllable concurrency limit, then report any failures.
#
# Concurrency is safe: zombienet native gives each network its own random tmp
# base dir / ports / node keys and injects --no-mdns, so concurrently-running
# networks stay isolated even though they share the same chain spec. The real
# limit is host CPU (PoW mining) and memory, hence the modest default below.
# Override with: JOBS=2 ./scripts/run-all.sh
set -euo pipefail
HARNESS="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HARNESS"

JOBS="${JOBS:-4}"
LOG_DIR="/tmp/zn-run-all-logs"
rm -rf "$LOG_DIR"
mkdir -p "$LOG_DIR"

# Auto-discover every scenario: any *.zndsl under scenarios/, run in sorted
# order. No manual list to keep in sync when adding a scenario.
mapfile -t SCENARIOS < <(find scenarios -name '*.zndsl' | sort)
if [[ ${#SCENARIOS[@]} -eq 0 ]]; then
    echo "No scenarios found under scenarios/" >&2
    exit 1
fi

# Turn a scenario path into a unique log-file slug (nested files are all named
# zombienet.zndsl, so basename alone would collide).
slug() {
    local s="${1#scenarios/}"
    s="${s%.zndsl}"
    echo "${s//\//-}"
}

# Adaptive scheduling: persist each scenario's last measured wall-clock time and
# run the slowest first (longest-processing-time-first), so a slow scenario does
# not become the tail that idles the other workers. Scenarios with no recorded
# time (e.g. brand-new ones) sort first, so an unknown cost is treated as slow.
DUR_DB="/tmp/zn-scenario-durations"
touch "$DUR_DB"
declare -A DUR
while read -r _sl _secs; do
    [[ -n "$_sl" ]] && DUR["$_sl"]="$_secs"
done < "$DUR_DB"

mapfile -t SCENARIOS < <(
    for s in "${SCENARIOS[@]}"; do
        printf '%s\t%s\n' "${DUR[$(slug "$s")]:-999999}" "$s"
    done | sort -rn | cut -f2-
)

echo "Running ${#SCENARIOS[@]} scenario(s), up to $JOBS in parallel (slowest first)."

# Run one scenario: capture full output to a per-scenario log, the pass/fail
# verdict to a sibling .status file, and the wall-clock seconds to a .dur file.
# Writing to files (instead of shared shell state) keeps the workers race-free.
run_one() {
    local s="$1"
    local sl; sl="$(slug "$s")"
    local start=$SECONDS
    echo ">>> start: $s"
    # The zombienet binary is a `pkg`-packaged Node app that extracts its native
    # addons into "$TMPDIR/pkg" on first launch. Concurrent launches race on
    # creating that shared dir and crash with EEXIST. Give each worker its own
    # TMPDIR so the extraction targets are isolated.
    local job_tmp="$LOG_DIR/tmp-$sl"
    mkdir -p "$job_tmp"
    if TMPDIR="$job_tmp" zombienet -p native test "$s" > "$LOG_DIR/$sl.log" 2>&1; then
        echo "PASS" > "$LOG_DIR/$sl.status"
        echo "<<< pass:  $s ($((SECONDS - start))s)"
    else
        echo "FAIL" > "$LOG_DIR/$sl.status"
        echo "<<< FAIL:  $s ($((SECONDS - start))s)"
    fi
    echo "$sl $((SECONDS - start))" > "$LOG_DIR/$sl.dur"
}

# Sliding-window scheduler: keep at most $JOBS workers running at once.
for s in "${SCENARIOS[@]}"; do
    while (( $(jobs -rp | wc -l) >= JOBS )); do
        wait -n
    done
    run_one "$s" &
done
wait

# Fold this run's measured durations into the persistent DB (new values win,
# untouched scenarios keep their previous time) so the next run orders better.
for s in "${SCENARIOS[@]}"; do
    f="$LOG_DIR/$(slug "$s").dur"
    if [[ -f "$f" ]]; then
        read -r _sl _secs < "$f"
        DUR["$_sl"]="$_secs"
    fi
done
for _sl in "${!DUR[@]}"; do
    echo "$_sl ${DUR[$_sl]}"
done > "$DUR_DB"

# Collect verdicts in scenario order.
fail=0
failed_list=()
for s in "${SCENARIOS[@]}"; do
    if [[ "$(cat "$LOG_DIR/$(slug "$s").status" 2>/dev/null)" != "PASS" ]]; then
        fail=$((fail + 1))
        failed_list+=("$s")
    fi
done

echo
if [[ $fail -eq 0 ]]; then
    echo "All scenarios passed."
else
    echo "$fail scenario(s) failed:"
    for f in "${failed_list[@]}"; do
        echo
        echo "=========================================================="
        echo "FAIL: $f"
        echo "=========================================================="
        log_file="$LOG_DIR/$(slug "$f").log"
        # Show failed assertions (❌ lines) with 3 lines of context above each
        grep -n "❌\|Result:" "$log_file" | head -40 || true
        echo "--- custom script output ---"
        grep -v "^[[:space:]]*$" "$log_file" | grep -v "^┌\|^│\|^└" | tail -20 || true
    done
    echo
    echo "Full logs saved to: $LOG_DIR"
    exit 1
fi
