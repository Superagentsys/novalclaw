# coding=utf-8
"""
Twitter 趋势抓取器

通过第三方 API 获取 Twitter/X 当前热门话题。
支持可配置的区域（WOEID）和最大条数。

设计原则：
- 输出格式与 DataFetcher.crawl_websites() 完全一致
- API 后端可替换（修改 _call_api() 即可）
- 失败不影响主流程（所有异常内部捕获）
- API Key 通过环境变量配置，不写入配置文件

可用的免费 API 选项：
- twitterapi.io：有免费 tier，REST API
- RapidAPI Twittr v2：500次/月免费
- RapidAPI twitter154：有免费 tier
"""

import json
import os
import random
import time
import urllib.parse
from typing import Dict, List, Optional, Tuple

import requests


# ════════════════════════════════════════════════════════
# WOEID 区域映射（人类可读名称 → 数字 WOEID）
# WOEID = Where On Earth IDentifier (Yahoo 地理编码)
# ════════════════════════════════════════════════════════
REGION_WOEID_MAP = {
    "worldwide": 1,
    "us": 23424977,
    "usa": 23424977,
    "united_states": 23424977,
    "japan": 23424856,
    "china": 23424781,
    "uk": 23424975,
    "united_kingdom": 23424975,
    "canada": 23424775,
    "australia": 23424748,
    "brazil": 23424768,
    "india": 23424848,
    "russia": 23424936,
    "germany": 23424829,
    "france": 23424819,
    "south_korea": 23424868,
    "mexico": 23424900,
    "argentina": 23424747,
    "turkey": 23424969,
    "indonesia": 23424846,
    "saudi_arabia": 23424938,
    "nigeria": 23424908,
    "south_africa": 23424942,
    "italy": 23424853,
    "spain": 23424950,
    "netherlands": 23424909,
    "philippines": 23424934,
    "thailand": 23424960,
}

# 存储用的 source_id 和显示名称
TWITTER_SOURCE_ID = "twitter-trends"
TWITTER_SOURCE_NAME = "Twitter Trending"

# 默认 API 端点（twitterapi.io 风格，可替换）
DEFAULT_API_URL = "https://api.twitterapi.io/twitter/trends"


def _resolve_woeid(region: str) -> int:
    """
    将地区名或 WOEID 解析为 WOEID 数字

    Args:
        region: 地区名（如 "worldwide", "japan"）或原始 WOEID 数字字符串

    Returns:
        WOEID 整数，解析失败时返回 1 (worldwide)
    """
    region = region.strip()
    # 纯数字 → 直接作为 WOEID
    if region.isdigit():
        return int(region)
    # 字符串 → 从映射表查找
    return REGION_WOEID_MAP.get(region.lower(), 1)


def _format_tweet_volume(volume: Optional[int]) -> str:
    """
    将推文数量格式化为人类可读的中文形式

    示例:
        12000 → " (1.2万讨论)"
        3500  → " (3.5千讨论)"
        500   → " (500讨论)"
        0     → ""
        None  → ""
    """
    if volume is None:
        return ""
    try:
        volume = int(volume)
    except (TypeError, ValueError):
        return ""
    if volume <= 0:
        return ""
    if volume >= 10000:
        return f" ({volume / 10000:.1f}万讨论)"
    elif volume >= 1000:
        return f" ({volume / 1000:.1f}千讨论)"
    else:
        return f" ({volume}讨论)"


class TwitterTrendsFetcher:
    """
    Twitter 趋势数据获取器

    通过第三方 API 获取 Twitter/X 当前热门话题。
    输出格式与 DataFetcher 完全一致，可无缝合并入热榜流水线。

    使用示例:
        fetcher = TwitterTrendsFetcher(
            api_key="your_key",
            region="japan",
            max_items=30,
        )
        results, source_id, source_name = fetcher.fetch()
        # results: {"twitter-trends": {"话题(1.2万讨论)": {"ranks":[1], ...}}}
    """

    MAX_RETRIES = 2

    def __init__(
        self,
        api_key: str = "",
        region: str = "worldwide",
        max_items: int = 50,
        api_url: str = "",
        proxy_url: Optional[str] = None,
    ):
        """
        Args:
            api_key: Twitter API Key（优先使用环境变量 TWITTER_API_KEY）
            region: 地区名或 WOEID（见 REGION_WOEID_MAP）
            max_items: 最大返回条数（1-50）
            api_url: 自定义 API 端点（留空使用默认）
            proxy_url: HTTP 代理 URL（可选）
        """
        self.api_key = api_key or os.environ.get("TWITTER_API_KEY", "")
        self.woeid = _resolve_woeid(region)
        self.max_items = min(max(max_items, 1), 50)
        self.api_url = api_url or DEFAULT_API_URL
        self.proxy_url = proxy_url

        # 日志前缀
        self._log_prefix = f"[Twitter] WOEID={self.woeid}"

    def fetch(self) -> Tuple[Dict, str, str]:
        """
        获取 Twitter 趋势数据

        Returns:
            (results, source_id, source_name) 元组：
            - results: 与 DataFetcher 输出格式完全一致
              {"twitter-trends": {"标题": {"ranks":[1], "url":"...", "mobileUrl":"..."}}}
            - source_id: "twitter-trends"
            - source_name: "Twitter Trending"
            失败时返回 ({}, source_id, source_name)
        """
        if not self.api_key:
            print(f"{self._log_prefix} [WARN] 未配置 API Key")
            print(f"  请设置环境变量 TWITTER_API_KEY，或在创建 Fetcher 时传入 api_key 参数")
            return {}, TWITTER_SOURCE_ID, TWITTER_SOURCE_NAME

        for attempt in range(self.MAX_RETRIES + 1):
            try:
                trends = self._call_api()

                if trends is None:
                    # API 调用失败
                    continue

                if not trends:
                    print(f"{self._log_prefix} API 返回空数据（该地区无趋势话题）")
                    return {}, TWITTER_SOURCE_ID, TWITTER_SOURCE_NAME

                results = self._convert_to_results(trends)
                count = len(results.get(TWITTER_SOURCE_ID, {}))
                print(f"{self._log_prefix} [OK] 获取 {count} 条趋势 (地区: {self._get_region_name()})")
                return results, TWITTER_SOURCE_ID, TWITTER_SOURCE_NAME

            except requests.Timeout:
                if attempt < self.MAX_RETRIES:
                    wait = random.uniform(2, 4)
                    print(f"{self._log_prefix} 请求超时 (第{attempt+1}次)，{wait:.1f}秒后重试...")
                    time.sleep(wait)
                else:
                    print(f"{self._log_prefix} [FAIL] 请求超时 (已达最大重试次数)")

            except requests.RequestException as e:
                if attempt < self.MAX_RETRIES:
                    wait = random.uniform(2, 5) * (attempt + 1)
                    print(f"{self._log_prefix} 请求失败 (第{attempt+1}次): {e}，{wait:.1f}秒后重试...")
                    time.sleep(wait)
                else:
                    print(f"{self._log_prefix} [FAIL] 请求失败 (已达最大重试次数): {e}")

            except (json.JSONDecodeError, KeyError, TypeError) as e:
                print(f"{self._log_prefix} [FAIL] API 响应格式异常: {e}")
                break  # 格式错误无需重试

            except Exception as e:
                print(f"{self._log_prefix} [FAIL] 未知错误: {e}")
                break

        return {}, TWITTER_SOURCE_ID, TWITTER_SOURCE_NAME

    def _get_region_name(self) -> str:
        """获取当前区域的可读名称（用于日志）"""
        for name, wid in REGION_WOEID_MAP.items():
            if wid == self.woeid:
                return name
        return str(self.woeid)

    def _call_api(self) -> Optional[List[Dict]]:
        """
        调用第三方 API 获取趋势数据

        默认适配 twitterapi.io 格式。
        需要适配其他 API 时，只需修改此方法的请求参数和响应解析逻辑。

        Returns:
            趋势条目列表，每项：{"name": str, "url": str, "tweet_volume": int or None}
            失败时返回 None
        """
        headers = {
            "X-API-Key": self.api_key,
            "Accept": "application/json",
            "User-Agent": "TrendRadar/6.9 TwitterFetcher",
        }

        params = {"woeid": self.woeid}

        proxies = None
        if self.proxy_url:
            proxies = {"http": self.proxy_url, "https": self.proxy_url}

        response = requests.get(
            self.api_url,
            headers=headers,
            params=params,
            proxies=proxies,
            timeout=15,
        )
        response.raise_for_status()

        data = response.json()

        # ── 适配多种 API 响应格式 ──
        # twitterapi.io 格式:  {"data": {"trends": [...]}}
        # 扁平格式:             {"trends": [...]}
        # 直接数组格式:          [...]
        if isinstance(data, dict):
            inner = data.get("data", data)
            if isinstance(inner, dict):
                trends = inner.get("trends", [])
            elif isinstance(inner, list):
                trends = inner
            else:
                trends = []
        elif isinstance(data, list):
            trends = data
        else:
            trends = []

        if trends and isinstance(trends, list):
            return trends[:self.max_items]

        return None

    def _convert_to_results(self, trends: List[Dict]) -> Dict:
        """
        将 API 返回的趋势数据转换为 TrendRadar 标准结果格式

        输出格式与 DataFetcher.crawl_websites() 完全一致：
        {
            "twitter-trends": {
                "话题名称 (1.2万讨论)": {
                    "ranks": [1],
                    "url": "https://twitter.com/search?q=...",
                    "mobileUrl": "https://twitter.com/search?q=..."
                }
            }
        }

        关键设计：
        - 推文讨论量追加到标题（因 SQLite schema 无通用 metadata 列）
        - 无 URL 的趋势自动生成 Twitter 搜索链接
        - 排名从 1 开始递增
        """
        results = {TWITTER_SOURCE_ID: {}}

        for index, trend in enumerate(trends, 1):
            # 跳过无效条目
            if not isinstance(trend, dict):
                continue

            name = trend.get("name", "").strip()
            if not name:
                continue

            # 推文量 → 人类可读后缀
            tweet_volume = trend.get("tweet_volume")
            volume_suffix = _format_tweet_volume(tweet_volume)
            display_title = f"{name}{volume_suffix}"

            # URL：优先使用 API 提供的，否则生成 Twitter 搜索链接
            url = trend.get("url", "").strip()
            if not url:
                encoded = urllib.parse.quote(name)
                url = f"https://twitter.com/search?q={encoded}&src=trend_click"

            mobile_url = trend.get("mobileUrl", "").strip() or url

            results[TWITTER_SOURCE_ID][display_title] = {
                "ranks": [index],
                "url": url,
                "mobileUrl": mobile_url,
            }

        return results

    @classmethod
    def from_config(cls, twitter_config: Dict, proxy_url: str = "") -> "TwitterTrendsFetcher":
        """
        从配置字典创建抓取器实例（工厂方法）

        Args:
            twitter_config: 配置字典，包含 REGION, MAX_ITEMS 等键
            proxy_url: 代理 URL（可选）

        Returns:
            TwitterTrendsFetcher 实例
        """
        return cls(
            region=twitter_config.get("REGION", "worldwide"),
            max_items=twitter_config.get("MAX_ITEMS", 50),
            proxy_url=proxy_url or None,
        )
