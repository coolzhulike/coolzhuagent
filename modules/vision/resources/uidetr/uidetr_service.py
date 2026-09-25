"""Small HTTP wrapper for racineai/UI-DETR-1 RF-DETR weights.

Run:
  python modules/vision/resources/uidetr/uidetr_service.py --model C:/path/to/model.pth --host 127.0.0.1 --port 7860

Contract:
  POST /detect
    {
      "image": "data:image/png;base64,...",
      "min_confidence": 0.35,
      "max_elements": 128
    }
"""

from __future__ import annotations

import argparse
import base64
import io
import os
from pathlib import Path
from typing import Any

import numpy as np
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from PIL import Image

try:
    from rfdetr.detr import RFDETRMedium
except Exception as exc:  # pragma: no cover - startup diagnostic
    RFDETRMedium = None
    RFDETR_IMPORT_ERROR = exc
else:
    RFDETR_IMPORT_ERROR = None


class DetectRequest(BaseModel):
    image: str
    model: str | None = None
    min_confidence: float | None = None
    max_elements: int | None = None


def decode_image(value: str) -> Image.Image:
    if not value:
        raise HTTPException(status_code=400, detail="image is required")
    if value.startswith("data:"):
        _, _, value = value.partition(",")
    try:
        data = base64.b64decode(value)
        return Image.open(io.BytesIO(data)).convert("RGB")
    except Exception as exc:
        raise HTTPException(status_code=400, detail=f"invalid image payload: {exc}") from exc


def detection_label(detections: Any, index: int) -> str:
    data = getattr(detections, "data", None) or {}
    for key in ("class_name", "label", "labels"):
        values = data.get(key) if isinstance(data, dict) else None
        if values is not None and index < len(values):
            label = str(values[index])
            return "ui-element" if label in {"", "__background__", "background"} else label
    class_ids = getattr(detections, "class_id", None)
    if class_ids is not None and index < len(class_ids):
        return f"class-{int(class_ids[index])}"
    return "ui-element"


def as_sequence(value: Any) -> list[Any]:
    if value is None:
        return []
    if hasattr(value, "tolist"):
        return value.tolist()
    return list(value)


def create_app(model_path: Path, resolution: int) -> FastAPI:
    app = FastAPI(title="UI-DETR-1 local detection service")
    state: dict[str, Any] = {"model": None}

    def get_model() -> Any:
        if RFDETRMedium is None:
            raise HTTPException(status_code=500, detail=f"rfdetr import failed: {RFDETR_IMPORT_ERROR}")
        if not model_path.exists():
            raise HTTPException(status_code=500, detail=f"model file not found: {model_path}")
        if state["model"] is None:
            state["model"] = RFDETRMedium(pretrain_weights=str(model_path), resolution=resolution)
        return state["model"]

    @app.get("/health")
    def health() -> dict[str, Any]:
        return {
            "ok": RFDETRMedium is not None and model_path.exists(),
            "model_path": str(model_path),
            "resolution": resolution,
            "loaded": state["model"] is not None,
            "import_error": str(RFDETR_IMPORT_ERROR) if RFDETR_IMPORT_ERROR else None,
        }

    @app.post("/detect")
    def detect(request: DetectRequest) -> dict[str, Any]:
        image = decode_image(request.image)
        threshold = float(request.min_confidence if request.min_confidence is not None else 0.35)
        max_elements = int(request.max_elements if request.max_elements is not None else 128)
        image_rgb = np.array(image)
        detections = get_model().predict(image_rgb, threshold=threshold)
        boxes = as_sequence(getattr(detections, "xyxy", None))
        scores = as_sequence(getattr(detections, "confidence", None))
        predictions = []
        for index, box in enumerate(boxes[:max_elements]):
            score = float(scores[index]) if index < len(scores) else 0.0
            x1, y1, x2, y2 = [float(value) for value in box]
            predictions.append(
                {
                    "box": {"xmin": x1, "ymin": y1, "xmax": x2, "ymax": y2},
                    "label": detection_label(detections, index),
                    "score": score,
                }
            )
        return {
            "image_width": image.width,
            "image_height": image.height,
            "predictions": predictions,
        }

    return app


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model", default=os.environ.get("UIDETR_MODEL_PATH", "model.pth"))
    parser.add_argument("--host", default=os.environ.get("UIDETR_HOST", "127.0.0.1"))
    parser.add_argument("--port", type=int, default=int(os.environ.get("UIDETR_PORT", "7860")))
    parser.add_argument("--resolution", type=int, default=int(os.environ.get("UIDETR_RESOLUTION", "1600")))
    args = parser.parse_args()

    import uvicorn

    app = create_app(Path(args.model), args.resolution)
    uvicorn.run(app, host=args.host, port=args.port, log_level="info")


if __name__ == "__main__":
    main()
