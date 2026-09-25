import sys, json, time, os, math, numpy as np

TARGET_SAMPLE_RATE = 16000

def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: whisper_transcribe.py <audio_file> [model] [language]"}))
        sys.exit(1)

    audio_file = sys.argv[1]
    model_name = sys.argv[2] if len(sys.argv) > 2 else "base"
    language = sys.argv[3] if len(sys.argv) > 3 else None

    if not os.path.exists(audio_file):
        print(json.dumps({"error": f"Audio file not found: {audio_file}"}))
        sys.exit(1)

    import whisper
    start = time.time()

    try:
        audio_data, sample_rate = load_audio(audio_file)
        audio_data = audio_data.astype(np.float32)
        audio_data = resample_audio(audio_data, sample_rate)

        # 模型放项目资源目录（web-console/models），随项目走、不散落在用户 ~/.cache。
        model_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "models")
        os.makedirs(model_dir, exist_ok=True)
        model = whisper.load_model(model_name, download_root=model_dir)
        result = model.transcribe(
            audio_data,
            language=language,
            verbose=False,
            fp16=False
        )
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)

    elapsed_ms = int((time.time() - start) * 1000)

    segments = []
    for seg in result.get("segments", []):
        segments.append({
            "text": seg["text"].strip(),
            "start_ms": int(seg["start"] * 1000),
            "end_ms": int(seg["end"] * 1000),
            "confidence": seg.get("confidence")
        })

    output = {
        "text": result["text"].strip(),
        "segments": segments,
        "duration_ms": elapsed_ms,
    }
    print(json.dumps(output))


def load_audio(path):
    ext = os.path.splitext(path)[1].lower()
    if ext in (".webm", ".ogg", ".opus", ".mp3", ".aac", ".m4a", ".flac", ".mp4"):
        return load_audio_av(path)
    return load_audio_sf(path)


def load_audio_sf(path):
    import soundfile as sf
    data, sr = sf.read(path, dtype="float32")
    if len(data.shape) > 1:
        data = data.mean(axis=1)
    data = np.asarray(data, dtype=np.float32)
    return data, sr


def load_audio_av(path):
    import av
    container = av.open(path)
    audio_stream = container.streams.audio[0]

    frames = []
    for frame in container.decode(audio_stream):
        arr = frame.to_ndarray()
        if len(arr.shape) > 1:
            arr = arr.mean(axis=0)
        frames.append(arr)

    if not frames:
        raise RuntimeError(f"No audio frames decoded from {path}")

    audio = np.concatenate(frames)
    audio = audio.astype(np.float32)

    if np.max(np.abs(audio)) > 1.0:
        audio = audio / 32768.0

    return audio, audio_stream.rate


def resample_audio(audio, source_sample_rate):
    if source_sample_rate == TARGET_SAMPLE_RATE:
        return audio.astype(np.float32)
    if source_sample_rate <= 0:
        raise RuntimeError(f"Invalid source sample rate: {source_sample_rate}")
    try:
        from scipy.signal import resample_poly
        gcd = math.gcd(int(source_sample_rate), TARGET_SAMPLE_RATE)
        up = TARGET_SAMPLE_RATE // gcd
        down = int(source_sample_rate) // gcd
        return resample_poly(audio, up, down).astype(np.float32)
    except Exception as exc:
        raise RuntimeError(
            f"Failed to resample audio from {source_sample_rate}Hz to {TARGET_SAMPLE_RATE}Hz: {exc}"
        )


if __name__ == "__main__":
    main()
