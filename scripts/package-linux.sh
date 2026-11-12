#!/usr/bin/env bash
# Fabrique l'archive d'installation Linux de Tsuki :
#   dist-linux/tsuki-<version>-linux-x86_64.tar.gz
# qui contient install.sh + un paquet par famille (.deb, .rpm, Arch, AppImage).
#
#   scripts/package-linux.sh            compile puis emballe
#   scripts/package-linux.sh --no-build emballe les paquets déjà compilés
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
BUNDLE="$ROOT/target/release/bundle"
NAME="tsuki-$VERSION-linux-$(uname -m)"
OUT="$ROOT/dist-linux/$NAME"

say() { printf '\n\033[1;35m==>\033[0m \033[1m%s\033[0m\n' "$*"; }

if [ "${1:-}" != "--no-build" ]; then
  say "Compilation de Tsuki $VERSION (deb, rpm, AppImage)"
  npm run tauri build
fi

DEB="$(ls "$BUNDLE"/deb/*.deb 2>/dev/null | head -1 || true)"
RPM="$(ls "$BUNDLE"/rpm/*.rpm 2>/dev/null | head -1 || true)"
APPIMAGE="$(ls "$BUNDLE"/appimage/*.AppImage 2>/dev/null | head -1 || true)"
[ -n "$DEB" ] || { echo "Paquet .deb introuvable dans $BUNDLE/deb" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"

# --- Paquet Arch, construit à partir du .deb (mêmes fichiers) ----------------
if command -v makepkg >/dev/null; then
  say "Paquet Arch (pacman)"
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
  cp "$DEB" "$WORK/tsuki.deb"
  sed "s/@VERSION@/$VERSION/" "$ROOT/packaging/linux/PKGBUILD.in" > "$WORK/PKGBUILD"
  # -d : les dépendances sont vérifiées à l'installation, pas ici.
  (cd "$WORK" && PKGDEST="$OUT" makepkg -f -d --noconfirm >/dev/null)
else
  echo "makepkg absent : pas de paquet Arch (à construire depuis une machine Arch)." >&2
fi

say "Assemblage de l'archive"
cp "$DEB" "$OUT/"
[ -n "$RPM" ] && cp "$RPM" "$OUT/"
[ -n "$APPIMAGE" ] && cp "$APPIMAGE" "$OUT/"
cp "$ROOT/src-tauri/icons/128x128@2x.png" "$OUT/tsuki.png"
install -m 755 "$ROOT/packaging/linux/install.sh" "$OUT/install.sh"
cp "$ROOT/packaging/linux/LISEZMOI.txt" "$OUT/"
(cd "$OUT" && sha256sum -- * > SHA256SUMS)

tar -C "$ROOT/dist-linux" -czf "$ROOT/dist-linux/$NAME.tar.gz" "$NAME"
say "Terminé : dist-linux/$NAME.tar.gz"
ls -lh "$OUT"
