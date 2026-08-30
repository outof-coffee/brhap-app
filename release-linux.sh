#!/usr/bin/env zsh
set -euo pipefail

# container is the native path on macOS; docker covers Linux dev
# workstations and macOS setups without container installed.
if command -v container >/dev/null 2>&1; then
  RUNTIME=container
elif command -v docker >/dev/null 2>&1; then
  RUNTIME=docker
else
  echo "error: neither 'container' nor 'docker' is installed" >&2
  exit 1
fi

echo "using container runtime: $RUNTIME"

# arm64 is the primary target; x86_64 is reachable via Rosetta translation
# on Apple Silicon, but is a secondary path since Apple has signaled Rosetta
# will not be around forever.
ARCH=${ARCH:-arm64}

# container and docker don't share flag syntax for cross-arch builds/runs.
# The image is tagged per arch so building one never clobbers the other's
# cached image.
if [ "$ARCH" = "x86_64" ]; then
  TRIPLE="x86_64-unknown-linux-gnu"
  if [ "$RUNTIME" = "container" ]; then
    ARCH_BUILD_FLAGS=(--arch x86_64)
    ARCH_RUN_FLAGS=(--arch x86_64 --rosetta)
  else
    ARCH_BUILD_FLAGS=(--platform linux/amd64)
    ARCH_RUN_FLAGS=(--platform linux/amd64)
  fi
else
  TRIPLE="aarch64-unknown-linux-gnu"
  ARCH_BUILD_FLAGS=()
  ARCH_RUN_FLAGS=()
fi
IMAGE="brhap-linux-builder-$ARCH"
OUT_DIR="target/$TRIPLE/release"

echo "target arch: $ARCH, out-dir: $OUT_DIR"

# scoped to brhap-app specifically, same as release.sh's macOS flow
VERSION=$(cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "brhap-app") | .version')

$RUNTIME build "${ARCH_BUILD_FLAGS[@]}" -t "$IMAGE" .

# Default container memory (1024 MB) OOM-kills rustc partway through this
# dependency tree, so it's raised explicitly here.
# --target is passed even though it matches the container's own native
# arch (this is not cross-compilation), so cargo writes to target/<triple>/
# release/ instead of target/release/ - the same per-target-directory
# convention release.sh already uses for macOS - and an arm64 and an x86_64
# build never share an output path.
$RUNTIME run --rm -m 6G "${ARCH_RUN_FLAGS[@]}" -v "$(pwd)":/work "$IMAGE" \
  cargo build --release -p brhap-app --target "$TRIPLE"

# cargo-packager's AppImage step uses std::filesystem::copy internally,
# which fails with a misleading "Permission denied" on the bind-mounted
# /work volume (virtiofs doesn't support the efficient-copy syscalls it
# tries). So staging happens on the container's own native filesystem
# instead - binaries-dir still points at the real (bind-mounted) build
# output, only out-dir moves off /work.
NATIVE_OUT="/root/appimage-out"

# No secret lives in this config (unlike macOS's signing identity), but it's
# still regenerated each run and gitignored to mirror release.sh's pattern.
# Named differently from packager.toml so the two scripts don't collide in
# the same working tree.
cat > packager-linux.toml <<EOF
# auto-generated - do not commit or edit!
name = "brhap-app"
product-name = "brhap"
version = "$VERSION"
identifier = "coffee.outof.brhap"
icons = ["brhap-app/assets/icon.png"]
out-dir = "$NATIVE_OUT"
binaries-dir = "$OUT_DIR"

[[binaries]]
path = "brhap-app"
main = true

[linux]
generate-desktop-entry = true

[appimage]
libs = [
  "libwayland-client.so.0",
  "libwayland-egl.so.1",
  "libxkbcommon.so.0",
  "libX11.so.6",
  "libX11-xcb.so.1",
  "libxcb.so.1",
]
EOF

# sys_admin is needed for linuxdeploy's FUSE-based AppImage extraction step.
# The final .AppImage is copied from the container's native out-dir back to
# the bind-mounted build output dir before the container (and its native
# filesystem) is removed.
$RUNTIME run --rm -m 6G --cap-add sys_admin "${ARCH_RUN_FLAGS[@]}" -v "$(pwd)":/work "$IMAGE" \
  sh -c "cargo packager -f appimage -c packager-linux.toml && cp $NATIVE_OUT/*.AppImage $OUT_DIR/"
