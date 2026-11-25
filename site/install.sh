#!/bin/sh
# Installs TermF from its latest GitHub release, checked against the release's SHA256SUMS.
#
#   curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh
#   curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh -s -- --beta
#
# Options: --beta (pre-releases too), --format appimage|deb|rpm (Linux; default: the system's package
# format, else the AppImage), --dry-run (say what would be installed, change nothing).
# macOS: TermF.app goes to /Applications (~/Applications when that is not writable), signed and notarized.
# Linux: the package through apt, dnf or zypper (sudo asks for your password), or the AppImage in ~/.local/bin.
set -eu

REPO=vassilidev/TermF
API=${TERMF_API:-https://api.github.com}
os=${TERMF_OS:-$(uname -s)}
arch=${TERMF_ARCH:-$(uname -m)}
format=${TERMF_FORMAT:-}
beta=0
dry=0

die() {
  printf 'TermF install: %s\n' "$*" >&2
  exit 1
}

while [ $# -gt 0 ]; do
  case $1 in
    --beta) beta=1 ;;
    --dry-run) dry=1 ;;
    --format)
      [ $# -gt 1 ] || die "--format needs appimage, deb or rpm"
      format=$2
      shift
      ;;
    *) die "unknown option: $1" ;;
  esac
  shift
done

case $arch in
  arm64 | aarch64) arch=aarch64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *) die "unsupported processor: $arch" ;;
esac

# Each asset is told apart by the end of its name.
case $os in
  Darwin)
    [ "$arch" = aarch64 ] && suffix=_aarch64.dmg || suffix=_x64.dmg
    ;;
  Linux)
    if [ -z "$format" ]; then
      if command -v dpkg >/dev/null 2>&1; then
        format=deb
      elif command -v rpm >/dev/null 2>&1; then
        format=rpm
      else
        format=appimage
      fi
    fi
    case $format-$arch in
      deb-x86_64) suffix=_amd64.deb ;;
      deb-aarch64) suffix=_arm64.deb ;;
      rpm-x86_64) suffix=.x86_64.rpm ;;
      rpm-aarch64) suffix=.aarch64.rpm ;;
      appimage-x86_64) suffix=_amd64.AppImage ;;
      appimage-aarch64) suffix=_aarch64.AppImage ;;
      *) die "unknown format: $format (appimage, deb or rpm)" ;;
    esac
    ;;
  *) die "unsupported system: $os (on Windows: irm https://vassilidev.github.io/TermF/install.ps1 | iex)" ;;
esac

release() {
  curl -fsSL -H 'Accept: application/vnd.github+json' "$API/repos/$REPO/$1"
}

# The latest release is the newest that is not a pre-release; until there is one, the newest pre-release.
if [ "$beta" = 1 ] || ! json=$(release releases/latest 2>/dev/null); then
  json=$(release 'releases?per_page=1') || die "cannot reach GitHub's API"
fi
fields() {
  printf '%s\n' "$json" | tr ',' '\n' | sed -n "s/.*\"$1\": *\"\\([^\"]*\\)\".*/\\1/p"
}
tag=$(fields tag_name | head -n 1)
[ -n "$tag" ] || die "no TermF release found"

pick() {
  fields browser_download_url | while read -r url; do
    case $url in *"$1") printf '%s\n' "$url" && break ;; esac
  done
}
asset=$(pick "$suffix")
[ -n "$asset" ] || [ "$os" != Darwin ] || asset=$(pick _universal.dmg)
[ -n "$asset" ] || die "TermF $tag has no file ending in $suffix"
sums=$(pick /SHA256SUMS)
[ -n "$sums" ] || die "TermF $tag publishes no SHA256SUMS: nothing to check the download against, so nothing is installed"
name=${asset##*/}

if [ "$dry" = 1 ]; then
  printf 'TermF %s\n  download %s\n  check it against %s\n' "$tag" "$asset" "$sums"
  exit 0
fi

tmp=$(mktemp -d)
cleanup() {
  [ ! -d "$tmp/mnt" ] || hdiutil detach -quiet "$tmp/mnt" 2>/dev/null || :
  rm -rf "$tmp"
}
trap cleanup EXIT
printf 'Downloading TermF %s (%s)…\n' "$tag" "$name"
curl -fL --progress-bar -o "$tmp/$name" "$asset"
curl -fsSL -o "$tmp/SHA256SUMS" "$sums"
expected=$(awk -v n="$name" '$2 == n || $2 == "*" n { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "$name is not listed in SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$tmp/$name" | cut -d ' ' -f 1)
else
  actual=$(shasum -a 256 "$tmp/$name" | cut -d ' ' -f 1)
fi
[ "$actual" = "$expected" ] || die "$name does not match its SHA-256 sum: not installed"

as_root() {
  if [ "$(id -u)" = 0 ]; then "$@"; else sudo "$@"; fi
}

case $os in
  Darwin)
    apps=${TERMF_APPS:-/Applications}
    [ -w "$apps" ] || apps=$HOME/Applications
    mkdir -p "$apps"
    mkdir "$tmp/mnt"
    hdiutil attach -nobrowse -readonly -quiet -mountpoint "$tmp/mnt" "$tmp/$name"
    # Copied beside the installed app first, then swapped: a failed copy leaves the old TermF in place.
    rm -rf "$apps/.TermF.app.new" "$apps/.TermF.app.old"
    ditto "$tmp/mnt/TermF.app" "$apps/.TermF.app.new"
    hdiutil detach -quiet "$tmp/mnt" && rmdir "$tmp/mnt"
    [ ! -e "$apps/TermF.app" ] || mv "$apps/TermF.app" "$apps/.TermF.app.old"
    if ! mv "$apps/.TermF.app.new" "$apps/TermF.app"; then
      [ ! -e "$apps/.TermF.app.old" ] || mv "$apps/.TermF.app.old" "$apps/TermF.app"
      die "cannot put TermF.app in $apps"
    fi
    rm -rf "$apps/.TermF.app.old"
    printf 'TermF %s is in %s.\n' "$tag" "$apps"
    ;;
  Linux)
    case $format in
      deb) as_root apt-get install -y "$tmp/$name" ;;
      rpm)
        if command -v dnf >/dev/null 2>&1; then
          as_root dnf install -y "$tmp/$name"
        elif command -v zypper >/dev/null 2>&1; then
          as_root zypper --non-interactive install --allow-unsigned-rpm "$tmp/$name"
        else
          as_root rpm -U "$tmp/$name"
        fi
        ;;
      appimage)
        mkdir -p "$HOME/.local/bin"
        cp "$tmp/$name" "$HOME/.local/bin/TermF.AppImage"
        chmod +x "$HOME/.local/bin/TermF.AppImage"
        printf 'TermF %s is ~/.local/bin/TermF.AppImage (it needs FUSE 2: libfuse2).\n' "$tag"
        ;;
    esac
    ;;
esac
