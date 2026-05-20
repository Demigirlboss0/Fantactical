#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BIN="./target/debug/ftctl"
echo "=== Building ftctl ==="
cargo build --bin ftctl -q 2>&1

pass=0
fail=0

check() {
    local label="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        echo "  PASS: $label"
        pass=$((pass + 1))
    else
        echo "  FAIL: $label — expected '$expected', got '$actual'"
        fail=$((fail + 1))
    fi
}

echo ""
echo "=== 1. Roll mechanics ==="
echo "--- Check-roll: 3d6 vs skill ---"
out=$("$BIN" check-roll 3 10 2>&1)
check "Natural 3 vs skill 10 => crit" "1" "$(echo "$out" | grep -c "CRITICAL")"

out=$("$BIN" check-roll 12 14 2>&1)
check "Roll 12 vs skill 14 => success" "1" "$(echo "$out" | grep -c "SUCCESS")"

out=$("$BIN" check-roll 15 14 2>&1)
check "Roll 15 vs skill 14 => failure" "1" "$(echo "$out" | grep -c "FAILURE")"

out=$("$BIN" check-roll 18 16 2>&1)
check "Natural 18 => crit fail" "1" "$(echo "$out" | grep -c "CRITICAL")"

echo "--- Half-skill crit range ---"
out=$("$BIN" check-roll 5 10 2>&1)
check "Roll 5 vs skill 10 (half=5) => crit" "1" "$(echo "$out" | grep -c "CRITICAL")"

out=$("$BIN" check-roll 6 12 2>&1)
check "Roll 6 vs skill 12 (half=6) => crit" "1" "$(echo "$out" | grep -c "CRITICAL")"

out=$("$BIN" check-roll 7 14 2>&1)
check "Roll 7 vs skill 14 (half=7) => crit" "1" "$(echo "$out" | grep -c "CRITICAL")"

echo ""
echo "=== 2. Injury resolution ==="
echo "--- Basic crush torso ---"
out=$("$BIN" test-injury --hp 12 --location torso --damage 5 --damage-type cr 2>&1)
check "Torso crush 5 → 5 injury" "5" "$(echo "$out" | grep "Raw injury" | grep -o '[0-9]*')"

echo "--- Skull impaling ---"
out=$("$BIN" test-injury --hp 12 --location skull --damage 6 --damage-type imp 2>&1)
check "Skull DR present" "2" "$(echo "$out" | grep "Effective DR" | grep -o '[0-9]*')"
check "Skull imp ×4" "4" "$(echo "$out" | grep "Wounding multiplier" | grep -o '[0-9.]*')"

echo "--- DR absorption ---"
out=$("$BIN" test-injury --hp 12 --location torso --damage 3 --damage-type cr --dr 5 2>&1)
check "DR 5 blocks 3 damage" "0" "$(echo "$out" | grep "Raw injury" | grep -o '[0-9]*')"

echo "--- Death check ---"
out=$("$BIN" test-injury --hp 10 --location torso --damage 50 --damage-type cr 2>&1)
check "50 damage kills HP10" "1" "$(echo "$out" | grep -c "Dead: true")"

echo "--- Eye illegal target ---"
out=$("$BIN" test-injury --hp 12 --location eye --damage 5 --damage-type cr 2>&1) || true
check "Eye crush rejected" "1" "$(echo "$out" | grep -c 'Error')"

echo ""
echo "=== 3. Available maneuvers ==="
echo "--- Standing ---"
out=$("$BIN" available-maneuvers 2>&1)
check "Standing has Attack" "1" "$(echo "$out" | grep -c Attack)"
check "Standing has Move" "1" "$(echo "$out" | grep -c Move)"

echo "--- Stunned ---"
out=$("$BIN" available-maneuvers --status stunned 2>&1)
check "Stunned only DoNothing" "1" "$(echo "$out" | grep -c "DoNothing")"

echo "--- Knocked down ---"
out=$("$BIN" available-maneuvers --status knocked_down 2>&1)
check "KD has ChangePosture" "1" "$(echo "$out" | grep -c "ChangePosture")"

echo "--- Prone ---"
out=$("$BIN" available-maneuvers --posture prone 2>&1)
move_count=$(echo "$out" | tr ',' '\n' | { grep -c '\bMove\b' || true; })
check "Prone can't Move" "0" "$move_count"

echo "--- Extra-heavy ---"
out=$("$BIN" available-maneuvers --encumbrance extraheavy 2>&1)
move_count=$(echo "$out" | tr ',' '\n' | { grep -c '\bMove\b' || true; })
check "Extra-heavy can't Move" "0" "$move_count"
atk_count=$(echo "$out" | tr ',' '\n' | { grep -c '\bAttack\b' || true; })
check "Extra-heavy can't Attack" "0" "$atk_count"

echo ""
echo "=== 4. Range & distance ==="
out=$("$BIN" range-penalty 1 2>&1)
check "Range 1 yd → penalty 0" "0" "$(echo "$out" | grep -o '\-*[0-9]*' | tail -1)"

out=$("$BIN" range-penalty 15 2>&1)
check "Range 15 yd → penalty -6" "-6" "$(echo "$out" | grep -o '\-*[0-9]*' | tail -1)"

out=$("$BIN" hex-distance 0 0 3 4 2>&1)
check "Hex distance (0,0)→(3,4)" "7" "$(echo "$out" | grep -o '[0-9]* yd' | grep -o '[0-9]*')"

echo ""
echo "=== 5. Save/load roundtrip ==="
tmp=$(mktemp -d)
"$BIN" save-state "$tmp/test.json" --round 3 2>&1 >/dev/null
out=$("$BIN" load-state "$tmp/test.json" 2>&1)
check "Save/load roundtrip" "3" "$(echo "$out" | grep -o 'round [0-9]*' | grep -o '[0-9]*')"
rm -rf "$tmp"

echo ""
echo "=== DICE (for visual inspection) ==="
"$BIN" dice -c 3
"$BIN" damage-roll 2 --adds 1

echo ""
echo "========================================"
echo "Results: $pass passed, $fail failed"
echo "========================================"
if [ "$fail" -gt 0 ]; then
    exit 1
fi
