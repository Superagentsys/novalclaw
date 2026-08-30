# coding=utf-8
"""
自定义数据源抓取模块

提供第三方 API 数据源（如 Twitter 趋势）的抓取功能。
与 newsnow 热榜管道输出统一的数据格式，无缝接入后续分析流水线。
"""

from trendradar.crawler.custom.twitter import TwitterTrendsFetcher

__all__ = ["TwitterTrendsFetcher"]
