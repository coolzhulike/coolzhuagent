# UI-DETR Detection Backend Contract

This folder documents the local/self-hosted detection service contract used by
`web-console` realtime perception. The model resource itself is not vendored in
the repository.

## Model Resource

- Hugging Face model: `racineai/UI-DETR-1`
- License: MIT
- Model card usage: RF-DETR-Medium, `model.predict(image_rgb, threshold=0.3)`
- Recommended default threshold from the model card: `0.35`

## HTTP Contract

Configure `coolzhu.toml`:

```toml
[vision.router.detection]
enabled = true
backend = "uidetr1"
model = "UI-DETR-1"
base_url = "http://127.0.0.1:7860"
fps = 2.0
min_confidence = 0.35
max_elements = 128
```

The web console maps `base_url` to `{base_url}/detect` unless the configured URL
already ends with `/detect` or `/detections`.

Request body:

```json
{
  "model": "UI-DETR-1",
  "image": "data:image/png;base64,...",
  "min_confidence": 0.35,
  "max_elements": 128
}
```

Supported response shapes include relative boxes:

```json
{
  "detections": [
    { "bbox": [0.1, 0.2, 0.3, 0.4], "label": "button", "text": "OK", "score": 0.91 }
  ]
}
```

And RF-DETR / Hugging Face style absolute boxes when image dimensions are sent:

```json
{
  "image_width": 1000,
  "image_height": 800,
  "predictions": [
    { "box": { "xmin": 100, "ymin": 160, "xmax": 300, "ymax": 320 }, "label": "button", "score": 0.88 }
  ]
}
```

## Runtime Flow

1. `POST /api/vision/realtime/start` starts the polling loop.
2. The loop captures the latest desktop screenshot.
3. The loop calls the configured UI-DETR-compatible `/detect` endpoint.
4. Parsed elements update the element table and are pushed over
   `GET /api/vision/realtime/events`.
5. `POST /api/vision/realtime/stop` aborts the loop and leaves the latest table
   available for inspection.

If `base_url` is not configured, start returns `waiting_for_detection_backend`
instead of starting a broken loop.

## Local Wrapper

This directory includes `uidetr_service.py`, a minimal FastAPI wrapper for
`racineai/UI-DETR-1` RF-DETR weights:

```powershell
python modules/vision/resources/uidetr/uidetr_service.py --model "C:\path\to\model.pth" --host 127.0.0.1 --port 7860
```

The wrapper is intentionally config driven. Pass the downloaded model path
through `--model` or `UIDETR_MODEL_PATH`; do not hardcode user-specific paths in
source.
