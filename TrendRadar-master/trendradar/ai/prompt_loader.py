# coding=utf-8
"""
提示词模板加载工具

从配置目录中加载 [system] / [user] 格式的提示词文件，
供 analyzer、translator、filter 等模块共享使用。
"""

from pathlib import Path
from typing import Tuple

# 项目 config 根目录
_CONFIG_ROOT = Path(__file__).parent.parent.parent / "config"


def load_prompt_template(
    prompt_file: str,
    config_subdir: str = "",
    label: str = "AI",
) -> Tuple[str, str]:
    """
    加载提示词模板文件，解析 [system] 和 [user] 部分。

    Args:
        prompt_file: 提示词文件名
        config_subdir: config 下的子目录（如 "ai_filter"），为空则直接在 config/ 下查找
        label: 日志标签，用于提示文件缺失时的打印

    Returns:
        (system_prompt, user_prompt_template) 元组
    """
    config_dir = _CONFIG_ROOT / config_subdir if config_subdir else _CONFIG_ROOT
    prompt_path = config_dir / prompt_file

    if not prompt_path.exists():
        print(f"[{label}] 提示词文件不存在: {prompt_path}")
        return "", ""

    content = prompt_path.read_text(encoding="utf-8")

    system_prompt = ""
    user_prompt = ""

    if "[system]" in content and "[user]" in content:
        parts = content.split("[user]")
        system_part = parts[0]
        user_part = parts[1] if len(parts) > 1 else ""

        if "[system]" in system_part:
            system_prompt = system_part.split("[system]")[1].strip()

        user_prompt = user_part.strip()
    else:
        user_prompt = content

    return system_prompt, user_prompt
