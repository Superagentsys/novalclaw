#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TrendRadar <-> OmniNova Claw MCP Bridge

Manage TrendRadar MCP Server lifecycle for OmniNova Gateway integration.

Usage:
    # Start MCP Server (HTTP mode for Gateway)
    python bridge.py start --port 3333

    # Start MCP Server (stdio mode for Agent)
    python bridge.py start --transport stdio

    # Health check
    python bridge.py health

    # Stop MCP Server
    python bridge.py stop

    # Trigger a crawl
    python bridge.py crawl

    # Get trending summary
    python bridge.py summary

Environment:
    TRENDRADAR_ROOT    TrendRadar project root (default: ../../TrendRadar-master)
    OMNINOVA_CONFIG    OmniNova config directory
"""

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Optional


def get_trendradar_root() -> Path:
    """获取 TrendRadar 项目根目录"""
    env_root = os.environ.get("TRENDRADAR_ROOT", "")
    if env_root:
        return Path(env_root)

    # 默认相对于此脚本的位置：skills/trendradar-news/ → ../../TrendRadar-master
    script_dir = Path(__file__).resolve().parent
    candidates = [
        script_dir / ".." / ".." / ".." / "TrendRadar-master",  # 同名仓库结构
        Path.cwd() / ".." / "TrendRadar-master",
        Path.home() / "TrendRadar",
    ]
    for candidate in candidates:
        if (candidate / "mcp_server" / "server.py").exists():
            return candidate.resolve()

    # 回退到脚本旁的同级目录
    return script_dir


def check_python() -> bool:
    """检查 Python 版本"""
    if sys.version_info < (3, 12):
        print("❌ 需要 Python 3.12+")
        return False
    return True


def check_dependencies(root: Path) -> bool:
    """检查 Python 依赖是否安装"""
    try:
        import fastmcp
        import feedparser
        import yaml
        return True
    except ImportError as e:
        print(f"❌ 缺少依赖: {e}")
        print(f"   请在 {root} 下执行: pip install -r requirements.txt")
        return False


def cmd_start(args):
    """启动 MCP Server"""
    root = get_trendradar_root()
    print(f"TrendRadar 根目录: {root}")

    if not check_python():
        sys.exit(1)
    if not check_dependencies(root):
        # 尝试自动安装
        req_file = root / "requirements.txt"
        if req_file.exists():
            print("正在自动安装依赖...")
            subprocess.run([sys.executable, "-m", "pip", "install", "-r", str(req_file)], check=False)

    transport = args.transport
    host = args.host
    port = args.port

    print(f"启动 TrendRadar MCP Server...")
    print(f"  传输模式: {transport}")

    if transport == "http":
        cmd = [
            sys.executable, "-m", "mcp_server",
            "--transport", "http",
            "--host", host,
            "--port", str(port),
            "--project-root", str(root),
        ]
        print(f"  监听地址: {host}:{port}")
        print(f"  MCP 端点: http://{host}:{port}/mcp")
    else:
        cmd = [
            sys.executable, "-m", "mcp_server",
            "--transport", "stdio",
            "--project-root", str(root),
        ]
        print("  模式: 标准输入输出")

    print()
    os.chdir(str(root))
    subprocess.run(cmd)


def cmd_health(args):
    """检查 MCP Server 健康状态"""
    import urllib.request
    import urllib.error

    url = f"http://{args.host}:{args.port}/mcp"
    try:
        req = urllib.request.Request(url, method="GET")
        with urllib.request.urlopen(req, timeout=5) as resp:
            print(f"✅ TrendRadar MCP Server 运行正常 ({url})")
            print(f"   状态码: {resp.status}")
    except urllib.error.URLError as e:
        print(f"❌ TrendRadar MCP Server 不可达 ({url})")
        print(f"   错误: {e.reason}")
        sys.exit(1)
    except Exception as e:
        print(f"❌ 健康检查失败: {e}")
        sys.exit(1)


def cmd_crawl(args):
    """触发一次爬取"""
    root = get_trendradar_root()
    print(f"[{time.strftime('%H:%M:%S')}] 触发爬取...")
    os.chdir(str(root))
    result = subprocess.run(
        [sys.executable, "-m", "trendradar"],
        capture_output=True, text=True, timeout=300
    )
    if result.returncode == 0:
        print("✅ 爬取完成")
        print(result.stdout[-500:] if len(result.stdout) > 500 else result.stdout)
    else:
        print(f"❌ 爬取失败 (exit code {result.returncode})")
        print(result.stderr[-500:])
        sys.exit(1)


def cmd_summary(args):
    """获取热点摘要"""
    root = get_trendradar_root()
    os.chdir(str(root))

    # 用 Python 直接调用 TrendRadar 工具
    code = f"""
import sys, json
sys.path.insert(0, '{root}')
from mcp_server.tools.data_query import DataQueryTools
from mcp_server.tools.analytics import AnalyticsTools
tools_data = DataQueryTools('{root}')
tools_analytics = AnalyticsTools('{root}')
topics = tools_data.get_trending_topics(top_n={args.top_n}, extract_mode='auto_extract')
print(json.dumps(topics, ensure_ascii=False, indent=2))
"""
    result = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True, text=True, timeout=60, cwd=str(root)
    )
    if result.returncode == 0:
        print(result.stdout)
    else:
        print("❌ 获取摘要失败")
        print(result.stderr)


def cmd_stop(args):
    """停止 MCP Server（发送 SIGTERM）"""
    import signal
    # 查找并终止 mcp_server 进程
    if sys.platform == "win32":
        subprocess.run(["taskkill", "/F", "/IM", "python.exe", "/FI", "WINDOWTITLE eq mcp_server*"], capture_output=True)
        print("已发送终止信号（Windows）")
    else:
        subprocess.run(["pkill", "-f", "mcp_server"], capture_output=True)
        print("已发送终止信号")


def main():
    parser = argparse.ArgumentParser(
        description="TrendRadar <-> OmniNova Claw MCP Bridge",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    sub = parser.add_subparsers(dest="command")

    # start
    p_start = sub.add_parser("start", help="Start MCP Server")
    p_start.add_argument("--transport", choices=["stdio", "http"], default="http")
    p_start.add_argument("--host", default="127.0.0.1")
    p_start.add_argument("--port", type=int, default=3333)

    # health
    p_health = sub.add_parser("health", help="Health check")
    p_health.add_argument("--host", default="127.0.0.1")
    p_health.add_argument("--port", type=int, default=3333)

    # crawl
    sub.add_parser("crawl", help="Trigger a crawl")

    # summary
    p_summary = sub.add_parser("summary", help="Get trending summary")
    p_summary.add_argument("--top-n", type=int, default=15)

    # stop
    sub.add_parser("stop", help="Stop MCP Server")

    args = parser.parse_args()

    if args.command == "start":
        cmd_start(args)
    elif args.command == "health":
        cmd_health(args)
    elif args.command == "crawl":
        cmd_crawl(args)
    elif args.command == "summary":
        cmd_summary(args)
    elif args.command == "stop":
        cmd_stop(args)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
