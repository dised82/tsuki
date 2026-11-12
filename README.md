# Tsuki 🌙

Lecteur **hors ligne** des VODs Twitch téléchargées avec l'application Android **Xtra**
(vidéo `.ts` + chat `_chat.json`), avec le chat synchronisé sur la vidéo.

## Fonctionnement

- **Aucune conversion** : Tsuki analyse le `.ts` une fois (≈ 1 s par heure de vidéo), repère les
  images clés, puis le sert à l'interface morceau par morceau. `hls.js` déballe le MPEG-TS à la
  volée et le navigateur intégré le décode.
- **Chat** : le JSON d'Xtra est lu même quand ses clés sont répétées (Xtra l'écrit par morceaux).
  Les emotes et badges inclus dans le fichier sont affichés.
- **Vérification** du chat, en trois niveaux :
  1. JSON valide ;
  2. chat Xtra ;
  3. correspondance avec la vidéo : heure de diffusion inscrite dans la vidéo (étiquettes ID3
     Twitch) = début du stream + `startTime` du chat, à 60 s près. Sinon, numéro de VOD dans le
     nom du fichier. Un chat qui ne correspond pas est **refusé**.

## Développement (Linux)

Dépendances système (Arch) :

```sh
sudo pacman -S --needed webkit2gtk-4.1 gst-libav gst-plugins-good gst-plugins-bad base-devel
```

Debian / Ubuntu :

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev librsvg2-dev \
  gstreamer1.0-libav gstreamer1.0-plugins-good gstreamer1.0-plugins-bad
```

Puis :

```sh
npm install
npm run tauri dev                      # lance Tsuki
TSUKI_OPEN=/chemin/video.ts npm run tauri dev   # ouvre directement une vidéo
npm run tauri build                    # paquets .deb et AppImage
cargo test                             # tests du cœur
cargo run --release --example inspect -- video.ts chat.json   # diagnostic
```

## Archive d'installation Linux

```sh
scripts/package-linux.sh     # → dist-linux/tsuki-<version>-linux-x86_64.tar.gz
```

L'archive contient un paquet par famille (`.deb`, `.rpm`, `.pkg.tar.zst` pour Arch, AppImage)
et `install.sh`, qui détecte la distribution (`/etc/os-release`) et installe le bon paquet avec
le gestionnaire du système, dépendances comprises :

```sh
tar xzf tsuki-0.1.0-linux-x86_64.tar.gz && cd tsuki-0.1.0-linux-x86_64
./install.sh               # ou --dry-run, --appimage, --yes, --uninstall
```

Le paquet Arch est fabriqué à partir du `.deb` : il faut lancer le script depuis une machine Arch
(`makepkg`).

## Problèmes connus

- **Fenêtre vide sous Linux** (machine virtuelle, pilote NVIDIA propriétaire) : le rendu DMA-BUF
  de WebKitGTK ne s'affiche pas. Tsuki le désactive automatiquement sur ces cartes graphiques ;
  sinon, lancer avec `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- **Emojis en carrés** dans le chat : installer une police d'emojis (`noto-fonts-emoji`).

## Organisation

| Dossier | Contenu |
|---|---|
| `crates/tsuki-core` | Analyse du `.ts`, lecture du chat, vérification (sans interface) |
| `src-tauri` | Application : commandes, protocole `tsuki://` (vidéo, emotes, badges) |
| `src` | Interface Svelte : lecteur, chat |

## Raccourcis

| Touche | Action |
|---|---|
| Espace / K | Lecture / pause |
| ← / → | −5 s / +5 s |
| J / L | −10 s / +10 s |
| ↑ / ↓ | Volume |
| F | Plein écran |
| M | Couper le son |
| T | Mode cinéma |
| C | Afficher / masquer le chat |
