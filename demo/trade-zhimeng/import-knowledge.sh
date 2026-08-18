#!/usr/bin/env bash
# 一键导入 trade-zhimeng 演示知识库（需已安装 omninova CLI 且 gateway 可不在线）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KNOWLEDGE_DIR="${SCRIPT_DIR}/knowledge"
COLLECTION="trade-zhimeng"

if ! command -v omninova >/dev/null 2>&1; then
  echo "错误: 未找到 omninova 命令。请先构建并加入 PATH："
  echo "  cargo build -p omninova-core --release --bin omninova"
  echo "  cp target/release/omninova ~/.local/bin/"
  exit 1
fi

echo "导入 collection: ${COLLECTION}"
echo "来源: ${KNOWLEDGE_DIR}"
echo

for f in "${KNOWLEDGE_DIR}"/*; do
  base="$(basename "$f")"
  title="${base%.*}"
  echo "→ ${title}"
  omninova knowledge add --title "${title}" --file "${f}" --collection "${COLLECTION}"
done

echo
echo "完成。验证检索："
omninova knowledge search "三齐操作" || true
echo
echo "下一步："
echo "  1. 将 persona.md 粘贴到 Web → Agents 系统提示"
echo "  2. omninova gateway run → http://127.0.0.1:10809/app"
echo "  3. 按 prompts/ 目录顺序演示"
