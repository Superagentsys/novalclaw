# coding=utf-8
"""
爬虫模块 - 数据抓取功能

提供两种数据源：
- DataFetcher: 通过 newsnow API 抓取多平台热榜数据
- TwitterTrendsFetcher: 通过第三方 API 获取 Twitter/X 趋势话题
"""

from trendradar.crawler.fetcher import DataFetcher
from trendradar.crawler.custom import TwitterTrendsFetcher

__all__ = ["DataFetcher", "TwitterTrendsFetcher"]
