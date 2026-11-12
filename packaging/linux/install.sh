#!/usr/bin/env bash
# Installation de Tsuki selon la distribution Linux.
#
#   ./install.sh              installe Tsuki
#   ./install.sh --uninstall  désinstalle Tsuki
#   ./install.sh --appimage   installe l'AppImage dans le dossier personnel (sans droits administrateur)
#   ./install.sh --yes        ne pose aucune question
#   ./install.sh --dry-run    affiche ce qui serait fait, sans rien faire
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACTION=install
MODE=auto
YES=0
DRY=0

for arg in "$@"; do
  case "$arg" in
    --uninstall) ACTION=uninstall ;;
    --appimage) MODE=appimage ;;
    --yes|-y) YES=1 ;;
    --dry-run) DRY=1 ;;
    -h|--help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Option inconnue : $arg (voir --help)" >&2; exit 2 ;;
  esac
done

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '\033[1;35m🌙\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m⚠\033[0m  %s\n' "$*" >&2; }
fail() { printf '\033[1;31m✖\033[0m  %s\n' "$*" >&2; exit 1; }

run() {
  printf '   \033[2m$ %s\033[0m\n' "$*"
  [ "$DRY" = 1 ] || "$@"
}

# Droits administrateur : root, sinon sudo, sinon pkexec.
SUDO=()
as_root() {
  if [ "${#SUDO[@]}" = 0 ] && [ "$(id -u)" != 0 ]; then
    if command -v sudo >/dev/null; then SUDO=(sudo)
    elif command -v pkexec >/dev/null; then SUDO=(pkexec)
    else fail "Il faut les droits administrateur (sudo ou pkexec introuvable). Essaie --appimage."
    fi
  fi
  run "${SUDO[@]}" "$@"
}

find_one() { # find_one 'motif' → chemin du fichier, ou rien
  local f
  for f in "$HERE"/$1; do [ -e "$f" ] && { echo "$f"; return; }; done
}

# --- Détection de la distribution ------------------------------------------
detect() {
  local ids="" osr="${TSUKI_OS_RELEASE:-/etc/os-release}"  # variable de test
  if [ -r "$osr" ]; then
    # shellcheck disable=SC1090
    . "$osr"
    ids=" ${ID:-} ${ID_LIKE:-} "
    DISTRO_NAME="${PRETTY_NAME:-${NAME:-Linux}}"
  else
    DISTRO_NAME="Linux"
  fi
  case "$ids" in
    *" arch "*|*" manjaro "*|*" endeavouros "*|*" cachyos "*|*" garuda "*) FAMILY=arch ;;
    *" debian "*|*" ubuntu "*) FAMILY=debian ;;
    *" fedora "*|*" rhel "*|*" centos "*) FAMILY=fedora ;;
    *" suse "*|*" opensuse "*|*" sles "*) FAMILY=suse ;;
    *)
      # Distribution inconnue : on se fie au gestionnaire de paquets présent.
      if command -v pacman >/dev/null; then FAMILY=arch
      elif command -v apt-get >/dev/null; then FAMILY=debian
      elif command -v dnf >/dev/null; then FAMILY=fedora
      elif command -v zypper >/dev/null; then FAMILY=suse
      else FAMILY=other
      fi ;;
  esac
}

confirm() {
  [ "$YES" = 1 ] && return 0
  read -r -p "   Continuer ? [O/n] " r
  case "${r,,}" in ""|o|oui|y|yes) return 0 ;; *) echo "Annulé."; exit 0 ;; esac
}

# --- Installation par famille ----------------------------------------------
install_arch() {
  local pkg; pkg="$(find_one 'tsuki-*.pkg.tar.zst')"
  [ -n "$pkg" ] || fail "Paquet Arch (tsuki-*.pkg.tar.zst) absent de l'archive."
  info "Installation du paquet Arch avec pacman (dépendances incluses)…"
  local flags=(-U); [ "$YES" = 1 ] && flags+=(--noconfirm)
  as_root pacman "${flags[@]}" "$pkg"
}

install_debian() {
  local pkg; pkg="$(find_one 'Tsuki_*.deb')"; [ -n "$pkg" ] || pkg="$(find_one '*.deb')"
  [ -n "$pkg" ] || fail "Paquet Debian (.deb) absent de l'archive."
  info "Installation du paquet Debian avec apt (dépendances incluses)…"
  as_root apt-get update
  local flags=(install); [ "$YES" = 1 ] && flags+=(-y)
  as_root apt-get "${flags[@]}" "$pkg"
}

install_fedora() {
  local pkg; pkg="$(find_one '*.rpm')"
  [ -n "$pkg" ] || fail "Paquet RPM absent de l'archive."
  info "Installation du paquet RPM avec dnf…"
  local y=(); [ "$YES" = 1 ] && y=(-y)
  as_root dnf install "${y[@]}" "$pkg"
  info "Modules vidéo GStreamer (H.264)…"
  as_root dnf install "${y[@]}" --skip-unavailable gstreamer1-plugins-good gstreamer1-plugins-bad-free \
    gstreamer1-plugin-libav gstreamer1-plugin-openh264 \
    || warn "Modules vidéo incomplets : si la vidéo reste noire, active RPM Fusion puis installe gstreamer1-libav."
}

install_suse() {
  local pkg; pkg="$(find_one '*.rpm')"
  [ -n "$pkg" ] || fail "Paquet RPM absent de l'archive."
  info "Installation du paquet RPM avec zypper…"
  local y=(); [ "$YES" = 1 ] && y=(--non-interactive)
  as_root zypper "${y[@]}" install --allow-unsigned-rpm "$pkg"
  as_root zypper "${y[@]}" install gstreamer-plugins-good gstreamer-plugins-bad gstreamer-plugins-libav \
    || warn "Modules vidéo incomplets : si la vidéo reste noire, ajoute le dépôt Packman."
}

APPIMAGE_DIR="$HOME/.local/share/tsuki"
DESKTOP_FILE="$HOME/.local/share/applications/tsuki.desktop"
ICON_FILE="$HOME/.local/share/icons/hicolor/256x256/apps/tsuki.png"

install_appimage() {
  local img; img="$(find_one '*.AppImage')"
  [ -n "$img" ] || fail "AppImage absente de l'archive."
  info "Installation de l'AppImage dans $APPIMAGE_DIR (sans droits administrateur)…"
  run mkdir -p "$APPIMAGE_DIR" "$(dirname "$DESKTOP_FILE")" "$(dirname "$ICON_FILE")" "$HOME/.local/bin"
  run install -m 755 "$img" "$APPIMAGE_DIR/Tsuki.AppImage"
  [ -f "$HERE/tsuki.png" ] && run install -m 644 "$HERE/tsuki.png" "$ICON_FILE"
  run ln -sf "$APPIMAGE_DIR/Tsuki.AppImage" "$HOME/.local/bin/tsuki"
  if [ "$DRY" = 0 ]; then
    cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Name=Tsuki
Comment=Lecteur hors ligne des VODs Twitch téléchargées avec Xtra
Exec=$APPIMAGE_DIR/Tsuki.AppImage %f
Icon=tsuki
Categories=AudioVideo;Video;Player;
MimeType=video/mp2t;
Terminal=false
EOF
  fi
  command -v update-desktop-database >/dev/null && run update-desktop-database "$(dirname "$DESKTOP_FILE")" || true
  warn "L'AppImage n'installe pas les modules vidéo GStreamer : s'ils manquent, la vidéo restera noire."
}

# --- Désinstallation -------------------------------------------------------
uninstall() {
  if [ -e "$APPIMAGE_DIR/Tsuki.AppImage" ]; then
    info "Suppression de l'AppImage…"
    run rm -f "$APPIMAGE_DIR/Tsuki.AppImage" "$HOME/.local/bin/tsuki" "$DESKTOP_FILE" "$ICON_FILE"
    run rmdir --ignore-fail-on-non-empty "$APPIMAGE_DIR"
  fi
  case "$FAMILY" in
    arch)   pacman -Q tsuki >/dev/null 2>&1 && as_root pacman -R tsuki ;;
    debian) dpkg -s tsuki >/dev/null 2>&1 && as_root apt-get remove tsuki ;;
    fedora) rpm -q tsuki >/dev/null 2>&1 && as_root dnf remove tsuki ;;
    suse)   rpm -q tsuki >/dev/null 2>&1 && as_root zypper remove tsuki ;;
  esac
  info "Tsuki est désinstallé. Ta bibliothèque et tes réglages sont conservés dans ~/.local/share/app.tsuki.player"
}

# --- Programme principal ---------------------------------------------------
detect
[ "$(uname -m)" = x86_64 ] || warn "Ces paquets sont compilés pour x86_64 ; ta machine est $(uname -m)."

if [ "$ACTION" = uninstall ]; then
  bold "Désinstallation de Tsuki — $DISTRO_NAME"
  [ "$DRY" = 1 ] && info "(simulation : rien ne sera modifié)"
  uninstall
  exit 0
fi

[ "$MODE" = appimage ] && FAMILY=appimage
case "$FAMILY" in
  arch)     LABEL="paquet Arch (pacman)" ;;
  debian)   LABEL="paquet Debian (apt)" ;;
  fedora)   LABEL="paquet RPM (dnf)" ;;
  suse)     LABEL="paquet RPM (zypper)" ;;
  *)        FAMILY=appimage; LABEL="AppImage dans le dossier personnel" ;;
esac

bold "Installation de Tsuki"
info "Distribution détectée : $DISTRO_NAME → $LABEL"
[ "$DRY" = 1 ] && info "(simulation : rien ne sera modifié)"
confirm
"install_$FAMILY"

if [ "$DRY" = 1 ]; then
  info "Simulation terminée."
elif command -v tsuki >/dev/null || [ -x "$APPIMAGE_DIR/Tsuki.AppImage" ]; then
  bold "✔ Tsuki est installé. Lance-le depuis le menu des applications ou avec la commande : tsuki"
else
  warn "Installation terminée, mais la commande « tsuki » est introuvable dans le PATH."
fi
