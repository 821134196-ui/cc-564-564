#!/usr/bin/env bash
# 诊所检查项目退项与费用调整系统 —— 一键启动
# 用法: ./start.sh   （首次运行会自动安装依赖并构建前端）
set -euo pipefail
cd "$(dirname "$0")"

export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
  echo "未找到 Rust 工具链，请先安装: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  exit 1
fi

echo "==> [1/3] 构建前端 (Svelte + Vite)"
if [ ! -d frontend/node_modules ]; then
  (cd frontend && npm install)
fi
(cd frontend && npm run build)

echo "==> [2/3] 编译后端 (Rust + Axum)"
cargo build

echo "==> [3/3] 启动服务  http://localhost:3000"
echo "    演示账号（密码均为 123456）: doctor / cashier / reporter / admin"
exec cargo run
