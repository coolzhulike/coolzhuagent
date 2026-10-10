"""核验总控导出的实际ACP页JSONL；不以本地生成的页冒充真实模型证据。"""
import argparse
import hashlib
import json
from pathlib import Path


def verify(pages):
    if not pages:
        raise ValueError("缺少实际页")
    first = pages[0]
    identity = tuple(first[k] for k in ("result_id", "sha256", "total_bytes", "source_tool"))
    cursor = 0
    restored = bytearray()
    for page in pages:
        if tuple(page[k] for k in ("result_id", "sha256", "total_bytes", "source_tool")) != identity:
            raise ValueError("分页身份发生变化")
        start, end = page["offset"], page["next_offset"]
        raw = page["result_text"].encode("utf-8")
        if start < 0 or end < start or end - start != len(raw) or end > identity[2]:
            raise ValueError("页偏移与正文长度不符")
        if start > cursor:
            raise ValueError(f"中段缺口：{cursor}→{start}")
        overlap = min(cursor - start, len(raw))
        if restored[start:start + overlap] != raw[:overlap]:
            raise ValueError("重读正文不一致")
        restored.extend(raw[overlap:])
        cursor = max(cursor, end)
        if page["complete"] != (end == identity[2]):
            raise ValueError("complete与尾页位置不符")
    if cursor != identity[2]:
        raise ValueError(f"尚未读齐：{cursor}/{identity[2]}")
    digest = hashlib.sha256(restored).hexdigest()
    if digest != identity[1]:
        raise ValueError("实际全部页重建摘要不符")
    return {"status": "contiguous_pages_verified", "result_id": identity[0],
            "total_bytes": cursor, "sha256": digest, "pages": len(pages),
            "limitation": "只证明导出的实际页完整；模型使用、Browser/DSH效果与跨重启须另核真实轨迹。"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pages", type=Path, help="同一result_id的实际页，按返回顺序，一行一页")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    pages = [json.loads(line) for line in args.pages.read_text(encoding="utf-8").splitlines() if line.strip()]
    try:
        result = verify(pages)
    except (ValueError, KeyError, TypeError) as error:
        result = {"status": "failed", "reason": str(error)}
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    if result["status"] == "failed":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
