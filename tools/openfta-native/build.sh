#!/usr/bin/env bash
# Build the original OpenFTA analysis engine as a headless program,
# `openfta-run`, for comparing ftar's reports with it (see README.md).
#
# usage: tools/openfta-native/build.sh [OUT_DIR]   (default: target/openfta-native)
# Needs git, gcc and a network connection for the first clone.
set -euo pipefail

# OpenFTA dev branch, "Update build and sources for JDK 21" (2026-11-18).
REPO=https://github.com/andy778/OpenFTA.git
COMMIT=fa753a9cef6006b35d17324f05a34b849ec5a66a

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../.." && pwd)
OUT=${1:-$ROOT/target/openfta-native}
SRC=$OUT/openfta
mkdir -p "$OUT/obj"

if [ ! -d "$SRC/.git" ]; then
  git clone -q "$REPO" "$SRC"
fi
git -C "$SRC" fetch -q origin "$COMMIT" 2>/dev/null || true
git -C "$SRC" -c advice.detachedHead=false checkout -q "$COMMIT"

LIB=$SRC/src/lib
INC=(-I"$LIB/Analysis" -I"$LIB/Appn" -I"$LIB/PrimaryEvents" -I"$LIB/Utilities"
     -I"$LIB/Canvas" -I"$LIB/Widgets" -I"$LIB/JNI")
# The 1990s C needs the old dialect and common symbols; warnings are noise here.
CFLAGS=(-std=gnu89 -w -O -fcommon ${OPENFTA_CFLAGS:-})

for f in "$LIB"/Analysis/*.c "$LIB"/Appn/*.c "$LIB"/PrimaryEvents/*.c \
         "$LIB"/Utilities/*.c "$LIB"/Canvas/*.c "$LIB"/Widgets/*.c; do
  case $(basename "$f") in
    "Copy of "*) continue ;;          # a stray backup in the tree
    PostScriptCanvas.c) continue ;;   # printing only; does not compile
  esac
  gcc "${CFLAGS[@]}" "${INC[@]}" -c "$f" -o "$OUT/obj/$(basename "$f" .c).o"
done
gcc "${CFLAGS[@]}" "${INC[@]}" -c "$HERE/driver.c" -o "$OUT/driver.o"
gcc "${CFLAGS[@]}" -c "$HERE/stubs.c" -o "$OUT/stubs.o"
gcc -fcommon -o "$OUT/openfta-run" "$OUT/driver.o" "$OUT/stubs.o" "$OUT"/obj/*.o -lm
echo "$OUT/openfta-run"
