#!/bin/sh
# Assert the upstream Brogue CE properties that late.sh's per-player save
# isolation rests on. Run against an unpacked Brogue CE source tree.
#
#   scripts/brogue_verify_isolation.sh upstream-brogue
#
# The door gives every player their own working directory and nothing else:
# same uid, same pod, one shared volume, no chroot (see the brogue door's
# CONTEXT.md section 4). That is only a wall because upstream offers a player no
# way to name a path outside the cwd. Nothing in our Rust test suite can see
# that, so it is checked here instead, fail-closed, in the image build that
# compiles the binary: a CE bump that relaxes any of it breaks the build rather
# than silently opening every save directory to every player.
#
# A failure here is not a reason to loosen the check. Re-audit the new source,
# and if a property genuinely moved, update both this script and CONTEXT.md
# section 4 in the same commit.
set -eu

SRC="${1:-.}"
GAME="$SRC/src/brogue"
PLATFORM="$SRC/src/platform"

fail() {
    echo "brogue isolation check FAILED: $1" >&2
    echo "  see late-ssh/src/app/door/brogue/CONTEXT.md section 4" >&2
    exit 1
}

test -d "$GAME" || fail "no brogue source at $GAME"
test -d "$PLATFORM" || fail "no platform source at $PLATFORM"

# 1. Path separators cannot reach a filename. Upstream substitutes '-' for each
#    of these in filename prompts, so "../someone/x" saves as "..-someone-x".
grep -qF "theChar == '/'" "$GAME/Recordings.c" \
    || fail "characterForbiddenInFilename no longer rejects '/'"
grep -qF "theChar == '\\\\'" "$GAME/Recordings.c" \
    || fail "characterForbiddenInFilename no longer rejects a backslash"
grep -qF "theChar == ':'" "$GAME/Recordings.c" \
    || fail "characterForbiddenInFilename no longer rejects ':'"

# 2. The filter is actually applied, on both the typed and the clipboard input
#    paths of getInputTextString. Two call sites; a drop to one means one path
#    stopped filtering.
applied=$(grep -cE 'characterForbiddenInFilename[[:space:]]*\(' "$GAME/IO.c")
[ "$applied" -eq 2 ] \
    || fail "expected 2 characterForbiddenInFilename call sites in IO.c, found $applied"

# 3. A player-named file always lands under a known suffix, so none can
#    overwrite something brogue reads at startup (keymap.txt above all).
grep -qF 'snprintf(filePath, BROGUE_FILENAME_MAX, "%s%s", filePathWithoutSuffix, GAME_SUFFIX)' \
    "$GAME/Recordings.c" \
    || fail "saveGame no longer appends GAME_SUFFIX to the entered name"

# 4. The load and recording browsers list only the working directory: no parent
#    navigation, no recursion, so no UI can name a file outside it.
grep -qF 'opendir ("./")' "$PLATFORM/platformdependent.c" \
    || fail "listFiles no longer scans only ./"

# 5. No shell escape: nothing in the game or platform sources spawns a process.
#    (We also pass an empty argv, which is what keeps the mode/seed flags out of
#    player reach; that one is enforced in late-brogue's host.rs.)
if grep -rEl '\b(system|popen|execl|execv|execvp|fork)[[:space:]]*\(' "$GAME" "$PLATFORM"; then
    fail "a process-spawning call appeared in the brogue sources listed above"
fi

echo "brogue isolation checks passed (5/5)"
