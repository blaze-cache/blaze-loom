#!/usr/bin/env sh
# Re-vendor blaze-cache-core from a pinned ../blaze-cache checkout (ADR-0002).
#
# Usage: sh scripts/sync-core.sh [path/to/blaze-cache] [--allow-dirty]
#
# The pinned commit lives here AND in docs/decisions/engine-provenance.md; this script
# asserts the two agree. See that file for the full rewrite list this script owns.
set -eu

PINNED=bf0bf2781e2f4ae34f0eab6b9776f79bc76fa8d1
REPO=../blaze-cache
ALLOW_DIRTY=0

for arg in "$@"; do
  case "$arg" in
    --allow-dirty) ALLOW_DIRTY=1 ;;
    -h|--help) sed -n '2,7p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) REPO="$arg" ;;
  esac
done

cd "$(dirname "$0")/.."
fail() { printf 'sync-core: %s\n' "$*" >&2; exit 1; }

[ -d "$REPO/.git" ] || [ -f "$REPO/.git" ] || fail "not a git checkout: $REPO"
upstream="$REPO/crates/blaze-cache-core"
[ -d "$upstream/src" ] || fail "no crates/blaze-cache-core/src in $REPO"

head=$(git -C "$REPO" rev-parse HEAD)
if [ "$head" != "$PINNED" ]; then
  if [ "$ALLOW_DIRTY" = 1 ]; then
    printf 'sync-core: WARNING: HEAD %s != pinned %s (--allow-dirty)\n' "$head" "$PINNED" >&2
  else
    fail "HEAD $head != pinned $PINNED (pass --allow-dirty to bypass)"
  fi
fi
if [ -n "$(git -C "$REPO" status --porcelain)" ]; then
  if [ "$ALLOW_DIRTY" = 1 ]; then
    printf 'sync-core: WARNING: upstream tree is dirty (--allow-dirty)\n' >&2
  else
    fail "upstream tree is dirty (pass --allow-dirty to bypass)"
  fi
fi

# The pin must be recorded in exactly one other place: the provenance record.
grep -q "$PINNED" docs/decisions/engine-provenance.md \
  || fail "pinned commit not found in docs/decisions/engine-provenance.md"

printf 'sync-core: copying %s @ %.12s\n' "$upstream" "$head"
mkdir -p src/engine/cache src/engine/eviction
cp "$upstream/src/lib.rs" src/engine/mod.rs
for f in bytes entry eviction node query stats tags; do
  cp "$upstream/src/$f.rs" "src/engine/$f.rs"
done
cp "$upstream/src/cache.rs" src/engine/cache.rs
cp "$upstream/src/cache/tests.rs" src/engine/cache/tests.rs
cp "$upstream/src/cache/stress.rs" src/engine/cache/stress.rs
cp "$upstream/src/cache/query_tests.rs" src/engine/cache/query_tests.rs
cp "$upstream/src/eviction/tests.rs" src/engine/eviction/tests.rs
cp "$upstream/tests/allocations.rs" tests/engine_allocations.rs

# --- mechanical rewrites: the only content changes (engine-provenance.md) ---
# Order matters: the crate-path rewrite runs first so the clock rewrite's fresh
# `crate::clock::` is not re-rewritten to `crate::engine::clock::`.
find src/engine tests/engine_allocations.rs -name '*.rs' -exec sh -c '
  for f do
    sed -i.bak \
      -e "s/crate::/crate::engine::/g" \
      -e "s/use blaze_cache_core::/use blaze_cache_wasm::engine::/g" \
      -e "s/web_time::Instant/crate::clock::Instant/g" \
      -e "s/on wasm that clock call is a \`performance.now()\`$/on wasm that call reads the host-ticked clock/" \
      -e "s/import, i.e. a boundary crossing per operation./(\`__blaze_tick\`), work per operation that avoids it./" \
      -e "s/On wasm \`Instant::now()\` is a \`performance.now()\` import — a$/On wasm \`Instant::now()\` reads the host-ticked clock/" \
      -e "s/boundary crossing per read — which is exactly what this avoids./(\`__blaze_tick\`) — work per read that this avoids./" \
      -e "s/wasm that call is a \`performance.now()\` import)/wasm that call reads the host-ticked clock)/" \
      -e "s/pay a wasm→JS \`performance.now()\` call just to discard the result./read the host-ticked clock just to discard the result./" \
      -e "s/on wasm that call is a \`performance.now()\`$/on wasm that call reads the host-ticked/" \
      -e "s/import, i.e. a JS boundary crossing per operation./clock (\`__blaze_tick\`), i.e. no import at all./" \
      -e "s/(a wasm→JS boundary$/(a host-clock read on/" \
      -e "s/crossing) when no entry/wasm) when no entry/" \
      "$f" && rm -f "$f.bak"
  done
' sh {} +

# --- assertions: nothing upstream-only may survive ---
if grep -rn 'web_time\|performance\.now\|blaze_cache_core' src/engine tests/engine_allocations.rs; then
  fail "stale upstream references remain (see engine-provenance.md)"
fi
if grep -rn 'crate::engine::engine::' src/engine; then
  fail "double-rewritten crate path"
fi

# --- format: upstream's style is not this repo's; the gate runs `cargo fmt --check` ---
# Run last so the script's fixed point is gate-clean: copy -> rewrite -> fmt is
# idempotent (rustfmt normalizes its own output on the next run).
cargo fmt --all

printf 'sync-core: done. state vs last commit:\n'
git status --short -- src/engine tests/engine_allocations.rs
git diff --stat -- src/engine tests/engine_allocations.rs || true
