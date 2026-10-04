#!/usr/bin/env bash
# Compile libtailscale (Tailscale embarque, API C) en .so Android.
# Prototype : le resultat n'est pas encore integre a l'APK. Voir README.md.
#
# Usage : ./build.sh [abi...]      (defaut : arm64-v8a x86_64)
# Pre-requis : go dans le PATH, NDK Android (ANDROID_NDK_HOME, ou le plus
# recent sous $ANDROID_HOME/ndk ou ~/Android/Sdk/ndk).
set -euo pipefail

# Commit de libtailscale valide avec ce script.
LIBTAILSCALE_REF="${LIBTAILSCALE_REF:-59d4bb82744915815178e0f0776d60026a397ee7}"
API_LEVEL=26

here="$(cd "$(dirname "$0")" && pwd)"
src="$here/.build/libtailscale"
out="$here/out"

if [ -z "${ANDROID_NDK_HOME:-}" ]; then
  sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
  ANDROID_NDK_HOME="$(ls -d "$sdk"/ndk/*/ 2>/dev/null | sort -V | tail -1)"
  ANDROID_NDK_HOME="${ANDROID_NDK_HOME%/}"
fi
[ -d "${ANDROID_NDK_HOME:-}" ] || { echo "NDK Android introuvable (definir ANDROID_NDK_HOME)" >&2; exit 1; }
command -v go >/dev/null || { echo "go introuvable dans le PATH" >&2; exit 1; }

if [ ! -d "$src/.git" ]; then
  mkdir -p "$(dirname "$src")"
  git clone -q https://github.com/tailscale/libtailscale.git "$src"
fi
git -C "$src" fetch -q origin
git -C "$src" checkout -q "$LIBTAILSCALE_REF"

# Go trop recent casse une dependance (go-json-experiment) : on suit go.mod.
export GOTOOLCHAIN="go$(awk '/^go /{print $2; exit}' "$src/go.mod")"
toolbin="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"

abis=("$@")
[ ${#abis[@]} -gt 0 ] || abis=(arm64-v8a x86_64)

for abi in "${abis[@]}"; do
  case "$abi" in
    arm64-v8a)   goarch=arm64; triple=aarch64-linux-android ;;
    x86_64)      goarch=amd64; triple=x86_64-linux-android ;;
    armeabi-v7a) goarch=arm;   triple=armv7a-linux-androideabi; export GOARM=7 ;;
    *) echo "ABI inconnue : $abi" >&2; exit 1 ;;
  esac
  mkdir -p "$out/$abi"
  echo ">> $abi ($GOTOOLCHAIN)"
  (cd "$src" && CGO_ENABLED=1 GOOS=android GOARCH="$goarch" CC="$toolbin/${triple}${API_LEVEL}-clang" \
    go build -buildmode=c-shared -o "$out/$abi/libtailscale.so" .)
  cp "$src/tailscale.h" "$out/tailscale.h"
done
ls -lh "$out"/*/libtailscale.so
