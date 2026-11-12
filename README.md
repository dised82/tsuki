# Tsuki 🌙

<img src="src-tauri/icons/128x128@2x.png" alt="" width="96" align="right">

Lecteur **hors ligne** des VODs Twitch téléchargées avec l'application Android **Xtra**
(vidéo `.ts` + chat `_chat.json`), avec le chat rejoué en même temps que la vidéo,
comme sur Twitch.

**Version actuelle : 0.3.0** — Linux. Windows et Android sont prévus.

## Fonctions

**Lecture**
- Lecture directe des `.ts` d'Xtra, **sans conversion**, même pour des vidéos de plusieurs Go.
- Reprise automatique là où tu t'étais arrêté.
- Plein écran de toute la fenêtre (vidéo + chat), mode cinéma, vitesse de 0,25× à 2×.

**Chat**
- Rejoué au fil de la vidéo, avec les emotes et badges inclus dans le fichier.
- Les messages arrivent par le bas ; faire défiler met le chat en pause.
- **Recherche** dans les messages et les pseudos (`/` ou `Ctrl+F`), `@pseudo` pour un
  utilisateur ; un clic sur un résultat amène la vidéo à ce moment.
- **Fiche utilisateur** : clic sur un pseudo → tous ses messages de la vidéo.
- Décalage réglable, heure des messages, taille du texte.

**Bibliothèque**
- Vidéos récentes sur l'écran d'accueil : miniature, durée, progression, titre, chaîne, catégorie.
- Recherche et tri (regardées récemment, date du stream, chaîne).
- Ajout de plusieurs vidéos à la fois, d'un **dossier entier** (sous-dossiers compris)
  ou par glisser-déposer.
- Le chat `<nom>_chat.json` posé à côté de la vidéo est chargé automatiquement.
- Une vidéo déplacée ou renommée est reconnue ; « Relocaliser » retrouve son emplacement.

## Installation (Linux)

Télécharger l'archive `tsuki-<version>-linux-x86_64.tar.gz`, puis :

```sh
tar xzf tsuki-0.3.0-linux-x86_64.tar.gz
cd tsuki-0.3.0-linux-x86_64
./install.sh               # ou --dry-run, --appimage, --yes, --uninstall
```

`install.sh` détecte la distribution et installe le bon paquet avec ses dépendances
(modules vidéo GStreamer compris) :

| Distribution | Paquet |
|---|---|
| Arch, Manjaro, EndeavourOS… | `.pkg.tar.zst` (pacman) |
| Debian, Ubuntu, Mint, Pop!_OS… | `.deb` (apt) |
| Fedora | `.rpm` (dnf) |
| openSUSE | `.rpm` (zypper) |
| Autre | AppImage dans le dossier personnel, sans droits administrateur |

La même commande met à jour une version déjà installée. La bibliothèque et les positions de
lecture sont conservées dans `~/.local/share/app.tsuki.player/`.

## Raccourcis clavier

La touche **?** les affiche dans l'application.

| Touche | Action |
|---|---|
| Espace / K | Lecture / pause |
| ← / → | −5 s / +5 s |
| J / L | −10 s / +10 s |
| 0 … 9 | Aller à 0 %, 10 % … 90 % |
| Début / Fin | Début / fin de la vidéo |
| < / > | Ralentir / accélérer |
| ↑ / ↓ | Volume |
| M | Couper le son |
| F, double-clic | Plein écran (Échap pour sortir) |
| T | Mode cinéma |
| C | Afficher / masquer le chat |
| / ou Ctrl+F | Rechercher dans le chat |

## Fonctionnement

- **Aucune conversion** : Tsuki construit une table de navigation du `.ts` (images clés), puis le
  sert à l'interface morceau par morceau ; `hls.js` déballe le MPEG-TS à la volée et le
  navigateur intégré le décode. Au-delà de 400 Mo, seuls des extraits du fichier sont lus
  (≈ 5 %) : une vidéo de 5,3 Go est analysée en quelques secondes. La table est mise en cache,
  la réouverture est immédiate.
- **Chat** : le JSON d'Xtra est lu même quand ses clés sont répétées (Xtra l'écrit par morceaux),
  et ses images base64 sans remplissage sont acceptées.
- **Vérification** du chat, en trois niveaux :
  1. JSON valide ;
  2. chat Xtra ;
  3. correspondance avec la vidéo : heure de diffusion inscrite dans la vidéo (étiquettes ID3
     Twitch) = début du stream + `startTime` du chat, à 60 s près. Sinon, numéro de VOD dans le
     nom du fichier. Un chat qui ne correspond pas est **refusé**.
- **Bibliothèque** : chaque vidéo est identifiée par une empreinte (taille + début et fin du
  fichier), ce qui permet de la reconnaître après un déplacement.

## Problèmes connus

- **Fenêtre vide sous Linux** (machine virtuelle, pilote NVIDIA propriétaire) : le rendu DMA-BUF
  de WebKitGTK ne s'affiche pas. Tsuki le désactive automatiquement sur ces cartes graphiques ;
  sinon, lancer avec `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
- **Emojis en carrés** dans le chat : installer une police d'emojis (`noto-fonts-emoji`).
- **Emotes 7TV / BTTV / FFZ** : affichées en texte si Xtra ne les a pas enregistrées dans le JSON.

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
npm run tauri dev                               # lance Tsuki
TSUKI_OPEN=/chemin/video.ts npm run tauri dev   # ouvre directement une vidéo
cargo test                                      # tests du cœur
cargo run --release --example inspect -- video.ts chat.json [--sparse|--full]   # diagnostic
scripts/package-linux.sh                        # archive d'installation → dist-linux/
```

Le paquet Arch est fabriqué à partir du `.deb` : `scripts/package-linux.sh` doit tourner sur une
machine Arch (`makepkg`).

## Organisation

| Dossier | Contenu |
|---|---|
| `crates/tsuki-core` | Analyse du `.ts`, lecture du chat, vérification (sans interface) |
| `src-tauri` | Application : commandes, bibliothèque, protocole `tsuki://` (vidéo, emotes, badges, miniatures) |
| `src` | Interface Svelte : bibliothèque, lecteur, chat |
| `packaging/linux`, `scripts` | Script d'installation multi-distributions, fabrication de l'archive |
