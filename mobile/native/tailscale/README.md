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

## APK de test autonome (`tsprobe`)

```bash
./build-probe.sh                      # -> out/tsprobe.apk (arm64-v8a, ~14 Mo)
TS_AUTHKEY=tskey-auth-... ./build-probe.sh   # cle prerenseignee : ne pas diffuser l'APK
adb install -r out/tsprobe.apk
```

Construit sans Gradle (aapt2, javac, d8, apksigner), signe avec le keystore de
debug du projet. L'app (`com.nestor.tsprobe`) demande une cle d'authentification
Tailscale (ephemere de preference), appelle `tailscale_up`, ouvre une connexion
TCP vers la cible (defaut `kanto.felis-ionian.ts.net:443`), puis affiche le
verdict et la fin des journaux de libtailscale. Le noeud s'appelle
`nestor-probe` et est ephemere.

## Suite envisagee

1. Lancer `tsprobe` sur un telephone et lire le verdict.
2. Si le tunnel fonctionne : pont JNI + proxy TCP loopback pour OkHttp, puis
   inclusion des `.so` dans `jniLibs`.
