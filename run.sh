#!/usr/bin/env bash
# 一条命令启动：构建 Svelte 前端 + Rust/Axum 后端，然后打开服务。
# 用法：./run.sh [--reset]
set -euo pipefail
cd "$(dirname "$0")"

PORT="${PORT:-8080}"
DB="clinic.db"

if [[ "${1:-}" == "--reset" ]]; then
  echo ">> 重置演示数据库 ${DB}"
  rm -f "${DB}"
fi

# 1) 前端（如尚未构建或源码有更新）
if [ ! -d frontend/node_modules ]; then
  echo ">> 安装前端依赖"
  (cd frontend && npm install)
fi
if [ ! -d frontend/dist ] || [ "${SKIP_FRONTEND:-}" != "1" ]; then
  echo ">> 构建 Svelte 前端"
  (cd frontend && npm run build)
fi

# 2) Rust 工具链（按需）
if ! command -v cargo >/dev/null 2>&1; then
  echo ">> 安装 Rust 工具链"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  source "$HOME/.cargo/env"
fi

# 3) 后端
echo ">> 编译并启动 Axum 后端 (http://localhost:${PORT})"
export PORT
exec cargo run
