#!/usr/bin/env bash
# Construit l'APK de test autonome `tsprobe` (sans Gradle) : un nœud Tailscale
# embarque (libtailscale) qui rejoint le tailnet et ouvre une connexion TCP.
#
# Usage : [TS_AUTHKEY=tskey-auth-...] ./build-probe.sh
# Sortie : out/tsprobe.apk (cle optionnelle prerenseignee dans l'APK : ne jamais le diffuser).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
ndk="${ANDROID_NDK_HOME:-$(ls -d "$sdk"/ndk/*/ | sort -V | tail -1)}"; ndk="${ndk%/}"
bt="$(ls -d "$sdk"/build-tools/*/ | sort -V | tail -1)"; bt="${bt%/}"
platform="$(ls -d "$sdk"/platforms/android-*/ | sort -V | tail -1)"; platform="${platform%/}"
android_jar="$platform/android.jar"
keystore="$here/../../android/app/debug.keystore"
abi=arm64-v8a

[ -f "$here/out/$abi/libtailscale.so" ] || "$here/build.sh" "$abi"

work="$here/.build/probe"
rm -rf "$work"; mkdir -p "$work/lib/$abi" "$work/classes" "$work/dex" "$work/assets"

echo ">> JNI (libprobe.so)"
"$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android26-clang" -shared -fPIC -O2 \
  -I"$here/out" "$here/probe/jni/probe.c" -L"$here/out/$abi" -ltailscale \
  -o "$work/lib/$abi/libprobe.so"
cp "$here/out/$abi/libtailscale.so" "$work/lib/$abi/"

echo ">> Java -> dex"
javac --release 11 -cp "$android_jar" -d "$work/classes" $(find "$here/probe/src" -name '*.java')
"$bt/d8" --release --lib "$android_jar" --min-api 26 --output "$work/dex" \
  $(find "$work/classes" -name '*.class')

echo ">> APK"
"$bt/aapt2" link -o "$work/base.apk" --manifest "$here/probe/AndroidManifest.xml" \
  -I "$android_jar" --min-sdk-version 26 --target-sdk-version 34
if [ -n "${TS_AUTHKEY:-}" ]; then printf '%s' "$TS_AUTHKEY" > "$work/assets/authkey.txt"; fi
python3 - "$work" "$abi" <<'PY'
import os, sys, zipfile
work, abi = sys.argv[1:3]
with zipfile.ZipFile(f"{work}/base.apk", "a", zipfile.ZIP_DEFLATED) as z:
    z.write(f"{work}/dex/classes.dex", "classes.dex")
    for so in sorted(os.listdir(f"{work}/lib/{abi}")):
        z.write(f"{work}/lib/{abi}/{so}", f"lib/{abi}/{so}")
    key = f"{work}/assets/authkey.txt"
    if os.path.exists(key):
        z.write(key, "assets/authkey.txt")
PY
"$bt/zipalign" -f -p 4 "$work/base.apk" "$work/aligned.apk"
mkdir -p "$here/out"
"$bt/apksigner" sign --ks "$keystore" --ks-pass pass:android --key-pass pass:android \
  --ks-key-alias androiddebugkey --out "$here/out/tsprobe.apk" "$work/aligned.apk"
"$bt/apksigner" verify "$here/out/tsprobe.apk" && ls -lh "$here/out/tsprobe.apk"
