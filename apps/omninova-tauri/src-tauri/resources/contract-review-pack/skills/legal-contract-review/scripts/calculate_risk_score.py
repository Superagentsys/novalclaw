#!/usr/bin/env python3
"""
合同风险评分计算工具

根据识别的风险条款计算合同的综合风险评分。
"""

import sys
import argparse
import json
from typing import List, Dict, Optional


def calculate_risk_score(
    high_risk_count: int = 0,
    medium_risk_count: int = 0,
    low_risk_count: int = 0,
    contract_type: Optional[str] = None,
    contract_amount: Optional[float] = None,
    partner_level: Optional[str] = None
) -> Dict[str, any]:
    """
    计算合同风险评分

    评分规则：
    - 基础分：100 分
    - 高风险条款：每个扣 20 分
    - 中风险条款：每个扣 10 分
    - 低风险条款：每个扣 5 分
    - 合同类型调整因子
    - 合同金额调整因子
    - 合作对象调整因子

    参数:
        high_risk_count: 高风险条款数量
        medium_risk_count: 中风险条款数量
        low_risk_count: 低风险条款数量
        contract_type: 合同类型 (nda/service/procurement/other)
        contract_amount: 合同金额
        partner_level: 合作对象等级 (strategic/premium/standard/basic)

    返回:
        评分结果字典：
        {
            "score": 风险评分 (0-100),
            "level": 风险等级 (high/medium/low/none),
            "details": 详细评分信息
        }
    """
    # 基础分
    base_score = 100

    # 风险条款扣分
    risk_deduction = (
        high_risk_count * 20 +
        medium_risk_count * 10 +
        low_risk_count * 5
    )

    # 合同类型调整因子
    contract_type_factor = 1.0
    if contract_type:
        type_factors = {
            "nda": 0.8,          # NDA 风险相对较低
            "service": 1.0,       # 服务合同标准风险
            "procurement": 0.9,   # 采购合同略低
            "cooperation": 1.1,   # 合作协议略高
            "other": 1.0
        }
        contract_type_factor = type_factors.get(contract_type.lower(), 1.0)

    # 合同金额调整因子
    amount_factor = 1.0
    if contract_amount:
        if contract_amount > 10000000:        # 1000 万以上
            amount_factor = 1.2
        elif contract_amount > 1000000:       # 100 万以上
            amount_factor = 1.1
        elif contract_amount > 100000:        # 10 万以上
            amount_factor = 1.05
        else:                                  # 10 万以下
            amount_factor = 1.0

    # 合作对象等级调整因子
    partner_factor = 1.0
    if partner_level:
        partner_factors = {
            "strategic": 0.8,    # 战略客户，风险较低
            "premium": 0.9,       # 优质客户
            "standard": 1.0,      # 标准客户
            "basic": 1.1          # 基础客户，风险较高
        }
        partner_factor = partner_factors.get(partner_level.lower(), 1.0)

    # 计算调整后的扣分
    adjusted_deduction = risk_deduction * contract_type_factor * amount_factor * partner_factor

    # 计算最终得分
    final_score = max(0, base_score - adjusted_deduction)

    # 确定风险等级
    if final_score < 60:
        risk_level = "high"
    elif final_score < 80:
        risk_level = "medium"
    elif final_score < 90:
        risk_level = "low"
    else:
        risk_level = "none"

    # 构建详细评分信息
    details = {
        "base_score": base_score,
        "risk_deduction": risk_deduction,
        "adjusted_deduction": round(adjusted_deduction, 2),
        "contract_type_factor": contract_type_factor,
        "amount_factor": amount_factor,
        "partner_factor": partner_factor,
        "risk_counts": {
            "high": high_risk_count,
            "medium": medium_risk_count,
            "low": low_risk_count
        }
    }

    return {
        "score": round(final_score, 2),
        "level": risk_level,
        "details": details
    }


def main():
    """命令行入口"""
    parser = argparse.ArgumentParser(
        description="计算合同风险评分"
    )
    parser.add_argument(
        "--high-risk",
        type=int,
        default=0,
        help="高风险条款数量"
    )
    parser.add_argument(
        "--medium-risk",
        type=int,
        default=0,
        help="中风险条款数量"
    )
    parser.add_argument(
        "--low-risk",
        type=int,
        default=0,
        help="低风险条款数量"
    )
    parser.add_argument(
        "--contract-type",
        choices=["nda", "service", "procurement", "cooperation", "other"],
        help="合同类型"
    )
    parser.add_argument(
        "--contract-amount",
        type=float,
        help="合同金额"
    )
    parser.add_argument(
        "--partner-level",
        choices=["strategic", "premium", "standard", "basic"],
        help="合作对象等级"
    )
    parser.add_argument(
        "--json-input",
        help="从 JSON 文件读取风险统计数据"
    )

    args = parser.parse_args()

    try:
        # 从 JSON 文件读取或使用命令行参数
        if args.json_input:
            with open(args.json_input, 'r', encoding='utf-8') as f:
                data = json.load(f)
                high_risk = data.get("high_risk_count", 0)
                medium_risk = data.get("medium_risk_count", 0)
                low_risk = data.get("low_risk_count", 0)
                contract_type = data.get("contract_type")
                contract_amount = data.get("contract_amount")
                partner_level = data.get("partner_level")
        else:
            high_risk = args.high_risk
            medium_risk = args.medium_risk
            low_risk = args.low_risk
            contract_type = args.contract_type
            contract_amount = args.contract_amount
            partner_level = args.partner_level

        # 计算风险评分
        result = calculate_risk_score(
            high_risk, medium_risk, low_risk,
            contract_type, contract_amount, partner_level
        )

        # 输出结果
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 0

    except Exception as e:
        print(f"错误: {str(e)}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
