# libtailscale pour Android (prototype)

Compile [libtailscale](https://github.com/tailscale/libtailscale) (Tailscale
embarque, API C, licence BSD-3) en `libtailscale.so` pour Android.

```bash
./build.sh                 # arm64-v8a et x86_64
./build.sh arm64-v8a       # une seule ABI
```

Sortie : `out/<abi>/libtailscale.so` et `out/tailscale.h` (ignores par git).
Pre-requis : `go` dans le PATH et le NDK Android. Le script suit la version de
Go de `go.mod` (Go 1.27 echoue sur `go-json-experiment`).

## Etat

- La compilation fonctionne (arm64-v8a : ~30 Mo).
- **Non integre a l'APK** et **connectivite non testee sur appareil**.
- Un wrapper Flutter signale que sur Android `net.Interfaces()` de Go peut
  bloquer le tunnel (`CAP_NET_ADMIN`) : a verifier en premier.

## Suite envisagee

1. Programme de test C (NDK) lance via `adb` : `tailscale_set_authkey` (cle
   ephemere), `tailscale_up`, `tailscale_dial("tcp", "kanto:8340")`.
2. Si le tunnel fonctionne : pont JNI + proxy TCP loopback pour OkHttp, puis
   inclusion des `.so` dans `jniLibs`.
