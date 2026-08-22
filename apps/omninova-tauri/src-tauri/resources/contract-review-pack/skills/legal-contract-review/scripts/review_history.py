#!/usr/bin/env python3
"""
审查历史管理工具

保存和查询合同审查历史记录，支持按供应商/合同类型/时间范围查询历史趋势。
"""

import sys
import argparse
import json
import os
from pathlib import Path
from datetime import datetime, timedelta
from typing import Dict, List, Optional

DEFAULT_HISTORY_DIR = Path(__file__).parent.parent / ".history"


def get_history_dir() -> Path:
    """获取历史记录存储目录"""
    history_dir = os.environ.get("CONTRACT_REVIEW_HISTORY_DIR", str(DEFAULT_HISTORY_DIR))
    return Path(history_dir)


def save_review_result(result: Dict, output_dir: Optional[str] = None) -> str:
    """
    保存一次审查记录

    参数:
        result: 审查结果字典，至少包含：
            - file_name: 审查的文件名
            - risk_score: 风险评分
            - risk_level: 风险等级
            - contract_type: 合同类型
            - parties: 签订方列表（可选）
        output_dir: 输出目录（可选，默认使用 .history/）

    返回:
        保存的文件路径
    """
    history_dir = Path(output_dir) if output_dir else get_history_dir()
    history_dir.mkdir(parents=True, exist_ok=True)

    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    safe_name = result.get("file_name", "unknown").replace(" ", "_")
    filename = f"{timestamp}_{safe_name}.json"
    filepath = history_dir / filename

    record = {
        "review_id": timestamp,
        "review_time": datetime.now().isoformat(),
        **result
    }

    with open(filepath, "w", encoding="utf-8") as f:
        json.dump(record, f, ensure_ascii=False, indent=2)

    return str(filepath)


def query_history(
    contract_type: Optional[str] = None,
    partner_name: Optional[str] = None,
    days: int = 365,
    risk_level: Optional[str] = None,
    history_dir: Optional[str] = None
) -> List[Dict]:
    """
    查询审查历史

    参数:
        contract_type: 合同类型过滤
        partner_name: 合作方名称过滤
        days: 查询最近N天的记录
        risk_level: 风险等级过滤
        history_dir: 历史记录目录

    返回:
        匹配的历史记录列表
    """
    hist_dir = Path(history_dir) if history_dir else get_history_dir()
    if not hist_dir.exists():
        return []

    cutoff = datetime.now() - timedelta(days=days)
    results = []

    for filepath in sorted(hist_dir.glob("*.json"), reverse=True):
        try:
            with open(filepath, "r", encoding="utf-8") as f:
                record = json.load(f)

            review_time = datetime.fromisoformat(record.get("review_time", "2000-01-01"))
            if review_time < cutoff:
                continue

            if contract_type and record.get("contract_type") != contract_type:
                continue

            if risk_level and record.get("risk_level") != risk_level:
                continue

            if partner_name:
                parties = record.get("parties", [])
                found = False
                if isinstance(parties, list):
                    for p in parties:
                        if isinstance(p, dict) and partner_name in p.get("name", ""):
                            found = True
                            break
                        elif isinstance(p, str) and partner_name in p:
                            found = True
                            break
                if not found:
                    continue

            results.append(record)

        except (json.JSONDecodeError, KeyError):
            continue

    return results


def get_trend(days: int = 365, history_dir: Optional[str] = None) -> Dict:
    """
    获取审查趋势分析

    参数:
        days: 分析最近N天
        history_dir: 历史记录目录

    返回:
        趋势分析结果
    """
    records = query_history(days=days, history_dir=history_dir)

    if not records:
        return {"message": "无历史数据", "total_reviews": 0}

    scores = [r.get("risk_score", 0) for r in records]
    levels = [r.get("risk_level", "unknown") for r in records]
    types = [r.get("contract_type", "unknown") for r in records]

    level_counts = {}
    for l in levels:
        level_counts[l] = level_counts.get(l, 0) + 1

    type_counts = {}
    for t in types:
        type_counts[t] = type_counts.get(t, 0) + 1

    first_review = datetime.fromisoformat(records[-1].get("review_time", ""))
    last_review = datetime.fromisoformat(records[0].get("review_time", ""))
    days_span = (last_review - first_review).days or 1

    return {
        "total_reviews": len(records),
        "period_days": days_span,
        "reviews_per_week": round(len(records) / max(days_span, 1) * 7, 1),
        "average_score": round(sum(scores) / len(scores), 1) if scores else 0,
        "min_score": min(scores) if scores else 0,
        "max_score": max(scores) if scores else 0,
        "risk_distribution": level_counts,
        "contract_type_distribution": type_counts,
    }


def main():
    """命令行入口"""
    parser = argparse.ArgumentParser(description="合同审查历史管理工具")
    subparsers = parser.add_subparsers(dest="command", help="命令")

    save_parser = subparsers.add_parser("save", help="保存审查记录")
    save_parser.add_argument("json_input", help="审查结果 JSON 文件路径")

    query_parser = subparsers.add_parser("query", help="查询审查历史")
    query_parser.add_argument("--contract-type", help="按合同类型过滤")
    query_parser.add_argument("--partner", help="按合作方名称过滤")
    query_parser.add_argument("--days", type=int, default=365, help="最近N天")
    query_parser.add_argument("--risk-level", choices=["high", "medium", "low", "none"], help="按风险等级过滤")

    trend_parser = subparsers.add_parser("trend", help="审查趋势分析")
    trend_parser.add_argument("--days", type=int, default=365, help="分析最近N天")

    args = parser.parse_args()

    try:
        if args.command == "save":
            with open(args.json_input, "r", encoding="utf-8") as f:
                record = json.load(f)
            filepath = save_review_result(record)
            print(json.dumps({"status": "saved", "path": filepath}, ensure_ascii=False))

        elif args.command == "query":
            results = query_history(
                contract_type=args.contract_type,
                partner_name=args.partner,
                days=args.days,
                risk_level=args.risk_level
            )
            print(json.dumps(results, ensure_ascii=False, indent=2))

        elif args.command == "trend":
            trend = get_trend(days=args.days)
            print(json.dumps(trend, ensure_ascii=False, indent=2))

        else:
            parser.print_help()
            return 1

        return 0

    except Exception as e:
        print(f"错误: {str(e)}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
