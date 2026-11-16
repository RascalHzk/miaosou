#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# P0/P1 构建闸门：在 Debian 10（buster, glibc 2.28）容器内验证并构建秒搜（miaosou）信创版。
#
# 为什么必须是 Debian 10：UOS V20 的 glibc 是 2.28；Debian 10 归档仓库提供的
# libwebkit2gtk-4.0-dev 为 2.38.6，与 UOS V20-1070 的 2.38.6.0-deepin1 基本同版本，
# 是当前能拿到的最接近目标机的构建基线。
#
# 用法：
#   STAGE=check  ./linux/scripts/build-debian10.sh   # 只编译检查（默认，最快暴露问题）
#   STAGE=test   ./linux/scripts/build-debian10.sh   # 单元测试 + 真实 inotify 事件流测试
#   STAGE=bench  ./linux/scripts/build-debian10.sh   # release 性能基准（合成 20 万文件）
#   STAGE=build  ./linux/scripts/build-debian10.sh   # release 二进制
#   STAGE=bundle ./linux/scripts/build-debian10.sh   # 额外打 .deb（会编译 tauri-cli，较慢）
#   REBUILD_IMAGE=1 ...                              # 强制重建 builder 镜像
#
# 可覆盖的环境变量：
#   CONTAINER_PROXY   容器访问外网用的代理（默认 Docker Desktop 内置代理）
#   APT_MIRROR        buster 归档源
#   RUST_TOOLCHAIN    与仓库 rust-toolchain.toml 一致
#   FRESH=1           丢弃 target 缓存卷
#
# 前置条件：前端产物 linux/ui/dist 必须先构建（dist 不入库）：
#   cd linux/ui && npm ci && npm run build
#
# 说明：
#   - builder 镜像只在首次构建（apt 依赖 + rustup 工具链），之后复用；
#   - cargo registry 与 target 放在命名卷里，二次运行可复用，且与 macOS 本机隔离；
#   - 本网络环境下 Docker 容器默认不走宿主 Clash（会被解析到 fake-ip），
#     因此必须注入容器代理，脚本已默认指向 Docker Desktop 的 http.docker.internal:3128。
# ---------------------------------------------------------------------------
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPTS_DIR="${REPO_ROOT}/linux/scripts"

PLATFORM="${PLATFORM:-linux/amd64}"
BUILDER_IMAGE="${BUILDER_IMAGE:-miaosou-builder:debian10-amd64}"
BASE_IMAGE="${BASE_IMAGE:-docker.m.daocloud.io/library/debian:buster}"
STAGE="${STAGE:-check}"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-nightly-2025-12-11}"
APT_MIRROR="${APT_MIRROR:-http://mirrors.aliyun.com/debian-archive}"
CONTAINER_PROXY="${CONTAINER_PROXY:-http://http.docker.internal:3128}"
FRESH="${FRESH:-0}"
REBUILD_IMAGE="${REBUILD_IMAGE:-0}"

# registry 与 bin 分别做缓存卷：前者复用 crate 下载，后者复用 cargo install 装好的 tauri-cli
REGISTRY_VOLUME="${REGISTRY_VOLUME:-miaosou_registry}"
BIN_VOLUME="${BIN_VOLUME:-miaosou_cargo_bin}"
BUILD_VOLUME="${BUILD_VOLUME:-miaosou_build}"

# --- 1. 按需构建 builder 镜像 ---
# 说明：本网络环境下 BuildKit 的构建容器访问不到宿主代理（apt 会解析到 Clash fake-ip），
#       而 `docker run` 容器可以，所以这里用「运行容器 → 安装 → commit」的方式建镜像。
#       需要走标准 CI 时可用 scripts/Dockerfile.debian10（在代理可达的环境里）。
PREP_CONTAINER="${PREP_CONTAINER:-miaosou-builder-prep}"

# 前置检查：前端产物不存在时直接报错（容器内没有 node，不会替您构建）
if [ ! -f "${REPO_ROOT}/linux/ui/dist/index.html" ]; then
  echo "缺少前端产物 linux/ui/dist/index.html" >&2
  echo "请先执行：cd linux/ui && npm ci && npm run build" >&2
  exit 2
fi

if [ "${REBUILD_IMAGE}" = "1" ] || ! docker image inspect "${BUILDER_IMAGE}" >/dev/null 2>&1; then
  echo "==> 构建 builder 镜像 ${BUILDER_IMAGE}（首次约 10~20 分钟）"
  docker rm -f "${PREP_CONTAINER}" >/dev/null 2>&1 || true

  docker run -d --name "${PREP_CONTAINER}" \
    --platform "${PLATFORM}" \
    -e "HTTP_PROXY=${CONTAINER_PROXY}" -e "HTTPS_PROXY=${CONTAINER_PROXY}" \
    -e "http_proxy=${CONTAINER_PROXY}" -e "https_proxy=${CONTAINER_PROXY}" \
    -e "NO_PROXY=localhost,127.0.0.1" -e "no_proxy=localhost,127.0.0.1" \
    "${BASE_IMAGE}" sleep infinity >/dev/null

  docker exec "${PREP_CONTAINER}" bash -lc "
    set -e
    export DEBIAN_FRONTEND=noninteractive
    export RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo

    echo 'deb ${APT_MIRROR}/debian buster main contrib non-free' > /etc/apt/sources.list
    echo 'deb ${APT_MIRROR}/debian-security buster/updates main contrib non-free' >> /etc/apt/sources.list
    printf 'Acquire::Check-Valid-Until \"false\";\n' > /etc/apt/apt.conf.d/99no-check-valid-until
    printf 'Acquire::Retries \"5\";\n' > /etc/apt/apt.conf.d/80-retries

    apt-get update -qq
    apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates file \
      libwebkit2gtk-4.0-dev libgtk-3-dev libsoup2.4-dev librsvg2-dev libssl-dev
    rm -rf /var/lib/apt/lists/*

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
    sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain '${RUST_TOOLCHAIN}' --no-modify-path
    rm -f /tmp/rustup-init.sh
  "

  docker commit "${PREP_CONTAINER}" "${BUILDER_IMAGE}" >/dev/null
  docker rm -f "${PREP_CONTAINER}" >/dev/null
  echo "==> builder 镜像完成: ${BUILDER_IMAGE}"
fi

if [ "${FRESH}" = "1" ]; then
  echo "==> FRESH=1：删除 target 缓存卷"
  docker volume rm -f "${BUILD_VOLUME}" >/dev/null 2>&1 || true
fi
docker volume create "${BUILD_VOLUME}" >/dev/null
docker volume create "${REGISTRY_VOLUME}" >/dev/null
docker volume create "${BIN_VOLUME}" >/dev/null

echo "==> repo:    ${REPO_ROOT}"
echo "==> image:   ${BUILDER_IMAGE} (${PLATFORM})"
echo "==> stage:   ${STAGE}"
echo "==> proxy:   ${CONTAINER_PROXY}"
echo

PROXY_ENV=()
if [ -n "${CONTAINER_PROXY}" ]; then
  PROXY_ENV=(
    -e "HTTP_PROXY=${CONTAINER_PROXY}"
    -e "HTTPS_PROXY=${CONTAINER_PROXY}"
    -e "http_proxy=${CONTAINER_PROXY}"
    -e "https_proxy=${CONTAINER_PROXY}"
    -e "NO_PROXY=localhost,127.0.0.1,::1"
    -e "no_proxy=localhost,127.0.0.1,::1"
  )
fi

docker run --rm \
  --platform "${PLATFORM}" \
  -v "${REPO_ROOT}:/src" \
  -v "${REGISTRY_VOLUME}:/opt/cargo/registry" \
  -v "${BIN_VOLUME}:/opt/cargo/bin" \
  -v "${BUILD_VOLUME}:/build" \
  -w /src/linux/src-tauri \
  -e STAGE="${STAGE}" \
  -e RUSTUP_HOME=/opt/rustup \
  -e CARGO_HOME=/opt/cargo \
  -e PATH=/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  -e CARGO_TARGET_DIR=/build/target \
  -e CARGO_TERM_COLOR=never \
  "${PROXY_ENV[@]}" \
  "${BUILDER_IMAGE}" \
  bash -lc '
    set -euo pipefail
    # 登录 shell 会重置 PATH，必须显式加回容器内工具链路径
    export PATH="/opt/cargo/bin:${PATH}"

    echo "==> 构建环境"
    uname -m
    ldd --version | head -1 || true
    cat /etc/debian_version
    dpkg-query -W -f="\${Package} \${Version}\n" libwebkit2gtk-4.0-37 libgtk-3-0 libsoup2.4-1 2>/dev/null || true
    rustc --version
    cargo --version

    case "${STAGE}" in
      check)
        echo "==> cargo check --features custom-protocol"
        cargo check --features custom-protocol
        ;;
      test)
        echo "==> cargo test --features custom-protocol（单元测试）"
        cargo test --features custom-protocol
        echo "==> cargo test --features custom-protocol -- --ignored（真实 inotify 事件流）"
        cargo test --features custom-protocol -- --ignored
        ;;
      bench)
        echo "==> cargo test --release --features custom-protocol perf_benchmark（合成大目录树）"
        cargo test --release --features custom-protocol perf_benchmark -- --ignored --nocapture
        ;;
      build)
        echo "==> cargo build --release --features custom-protocol"
        cargo build --release --features custom-protocol
        ls -lh /build/target/release/miaosou
        ;;
      bundle)
        echo "==> cargo build --release --features custom-protocol"
        cargo build --release --features custom-protocol
        echo "==> tauri-cli 1.x（v1 打包需要）"
        if ! command -v cargo-tauri >/dev/null 2>&1; then
          cargo install tauri-cli --version "^1.5" --locked
        fi
        echo "==> cargo tauri build --bundles deb"
        cargo tauri build --bundles deb
        echo "==> 产物"
        ls -lh /build/target/release/bundle/deb/ || true
        echo "==> 校验 Depends（v1 的 deb.depends 是追加语义，默认依赖已含 webkit/gtk）"
        dpkg -I /build/target/release/bundle/deb/*.deb 2>/dev/null | grep -E "Package|Version|Depends|Architecture" || true
        ;;
      *)
        echo "未知 STAGE: ${STAGE}（可选 check / build / bundle）" >&2
        exit 2
        ;;
    esac

    echo
    echo "==> STAGE=${STAGE} 完成"
  '
