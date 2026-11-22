#!/bin/sh
# Dry runs of site/install.sh and site/install.ps1 against a fake GitHub API on localhost: the file each system
# and processor gets, the beta channel, the fallback to a pre-release, and the refusal without SHA256SUMS.
set -eu

root=$(cd "$(dirname "$0")/../.." && pwd)
python=$(command -v python3 || command -v python)
tmp=$(mktemp -d)
port=$((20000 + $$ % 20000))
api="http://127.0.0.1:$port"

# A release `v$1`, pre-release or not (`$2`), with SHA256SUMS or not (`$3`: yes / no), its files under `$4`.
assets() {
  v=$1
  base=${4:-https://github.com/vassilidev/TermF/releases/download/v$v}
  printf '{"tag_name": "v%s", "prerelease": %s, "assets": [' "$v" "$2"
  sums=$3
  set -- "TermF_${v}_aarch64.dmg" "TermF_${v}_x64.dmg" "TermF_${v}_universal.dmg" "TermF_${v}_amd64.deb" \
    "TermF_${v}_arm64.deb" "TermF-${v}-1.x86_64.rpm" "TermF-${v}-1.aarch64.rpm" "TermF_${v}_amd64.AppImage" \
    "TermF_${v}_aarch64.AppImage" "TermF_${v}_x64-setup.exe" "TermF_${v}_arm64-setup.exe" \
    "TermF_${v}_x64_en-US.msi" "TermF_aarch64.app.tar.gz" "latest.json"
  [ "$sums" = no ] || set -- "$@" SHA256SUMS
  sep=
  for name in "$@"; do
    printf '%s{"name": "%s", "browser_download_url": "%s/%s"}' "$sep" "$name" "$base" "$name"
    sep=,
  done
  printf ']}'
}

# `releases/latest` is a file, `releases` a folder whose index answers `releases?per_page=1`.
serve() {
  rm -rf "$tmp/api"
  mkdir -p "$tmp/api/repos/vassilidev/TermF/releases"
  [ -z "$1" ] || printf '%s' "$1" > "$tmp/api/repos/vassilidev/TermF/releases/latest"
  printf '[%s]' "$2" > "$tmp/api/repos/vassilidev/TermF/releases/index.html"
}

"$python" -m http.server "$port" --bind 127.0.0.1 --directory "$tmp/api" > "$tmp/server.log" 2>&1 &
server=$!
trap 'status=$?; kill $server 2>/dev/null || :; wait $server 2>/dev/null || :; rm -rf "$tmp"; exit $status' EXIT
serve "$(assets 0.2.0 false yes)" "$(assets 0.3.0-beta.1 true yes)"
for _ in 1 2 3 4 5 6 7 8 9 10; do
  curl -fs "$api/repos/vassilidev/TermF/releases/latest" > /dev/null 2>&1 && break
  sleep 1
done

fail=0
expect() {
  want=$1
  shift
  if out=$(env TERMF_API="$api" "$@" 2>&1) && printf '%s' "$out" | grep -q -- "$want"; then
    printf 'ok   %s → %s\n' "$*" "$want"
  else
    printf 'FAIL %s: wanted %s, got:\n%s\n' "$*" "$want" "$out"
    fail=1
  fi
}
refuse() {
  if out=$(env TERMF_API="$api" "$@" 2>&1); then
    printf 'FAIL %s: should have refused, got:\n%s\n' "$*" "$out"
    fail=1
  else
    printf 'ok   %s refused: %s\n' "$*" "$out"
  fi
}

sh=$root/site/install.sh
expect TermF_0.2.0_aarch64.dmg TERMF_OS=Darwin TERMF_ARCH=arm64 sh "$sh" --dry-run
expect TermF_0.2.0_x64.dmg TERMF_OS=Darwin TERMF_ARCH=x86_64 sh "$sh" --dry-run
expect TermF_0.2.0_amd64.deb TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --dry-run --format deb
expect TermF_0.2.0_arm64.deb TERMF_OS=Linux TERMF_ARCH=aarch64 sh "$sh" --dry-run --format deb
expect TermF-0.2.0-1.x86_64.rpm TERMF_OS=Linux TERMF_ARCH=amd64 sh "$sh" --dry-run --format rpm
expect TermF-0.2.0-1.aarch64.rpm TERMF_OS=Linux TERMF_ARCH=arm64 sh "$sh" --dry-run --format rpm
expect TermF_0.2.0_amd64.AppImage TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --dry-run --format appimage
expect TermF_0.2.0_aarch64.AppImage TERMF_OS=Linux TERMF_ARCH=aarch64 sh "$sh" --dry-run --format appimage
expect releases/download/v0.2.0/SHA256SUMS TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --dry-run --format deb
expect TermF_0.3.0-beta.1_aarch64.dmg TERMF_OS=Darwin TERMF_ARCH=arm64 sh "$sh" --dry-run --beta
refuse TERMF_OS=Darwin TERMF_ARCH=ppc sh "$sh" --dry-run
refuse TERMF_OS=FreeBSD TERMF_ARCH=x86_64 sh "$sh" --dry-run
refuse TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --dry-run --format snap
case $(uname -s) in
  Darwin | Linux) expect "download https://github.com/" sh "$sh" --dry-run ;;
esac

ps1=$root/site/install.ps1
if command -v pwsh > /dev/null 2>&1; then
  expect TermF_0.2.0_x64-setup.exe TERMF_ARCH=X64 TERMF_DRY_RUN=1 pwsh -NoProfile -File "$ps1"
  expect TermF_0.2.0_arm64-setup.exe TERMF_ARCH=Arm64 TERMF_DRY_RUN=1 pwsh -NoProfile -File "$ps1"
  expect TermF_0.3.0-beta.1_x64-setup.exe TERMF_ARCH=X64 TERMF_BETA=1 TERMF_DRY_RUN=1 pwsh -NoProfile -File "$ps1"
  expect "download https://github.com/" TERMF_DRY_RUN=1 pwsh -NoProfile -File "$ps1"
else
  echo "pwsh not found: install.ps1 not tried"
fi

# No stable release yet: the newest pre-release. No SHA256SUMS: nothing installed.
serve "" "$(assets 0.1.0 true yes)"
expect TermF_0.1.0_amd64.AppImage TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --dry-run --format appimage
serve "$(assets 0.2.0 false no)" "$(assets 0.2.0 false no)"
refuse TERMF_OS=Darwin TERMF_ARCH=arm64 sh "$sh" --dry-run
if command -v pwsh > /dev/null 2>&1; then
  refuse TERMF_ARCH=X64 TERMF_DRY_RUN=1 pwsh -NoProfile -File "$ps1"
fi

# A real install, of an AppImage into a throwaway home: checked against its sum, refused when it differs.
serve "$(assets 0.4.0 false yes "$api/dl")" "[]"
mkdir -p "$tmp/api/dl" "$tmp/home"
printf 'fake AppImage\n' > "$tmp/api/dl/TermF_0.4.0_amd64.AppImage"
(cd "$tmp/api/dl" && "$python" -c "import hashlib,sys; d=open(sys.argv[1],'rb').read(); print(hashlib.sha256(d).hexdigest() + '  ' + sys.argv[1])" TermF_0.4.0_amd64.AppImage > SHA256SUMS)
expect "is ~/.local/bin/TermF.AppImage" HOME="$tmp/home" TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --format appimage
if cmp -s "$tmp/api/dl/TermF_0.4.0_amd64.AppImage" "$tmp/home/.local/bin/TermF.AppImage"; then
  echo "ok   AppImage installed"
else
  echo "FAIL AppImage not installed"
  fail=1
fi
rm -rf "${tmp:?}/home"
printf 'tampered\n' > "$tmp/api/dl/TermF_0.4.0_amd64.AppImage"
refuse HOME="$tmp/home" TERMF_OS=Linux TERMF_ARCH=x86_64 sh "$sh" --format appimage
[ ! -e "$tmp/home/.local/bin/TermF.AppImage" ] || { echo "FAIL tampered AppImage installed"; fail=1; }

if command -v shellcheck > /dev/null 2>&1; then
  if shellcheck "$sh" "$0"; then echo "ok   shellcheck"; else fail=1; fi
else
  echo "shellcheck not found: not run"
fi
exit $fail
