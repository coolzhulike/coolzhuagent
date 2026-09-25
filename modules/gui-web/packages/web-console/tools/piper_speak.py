import sys, json, time, os, wave, io

# Force UTF-8 output to avoid GBK encoding errors on Windows
if sys.platform == 'win32':
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8')


def pitch_shift_preserve_duration(raw_bytes, semitones):
    """音调偏移但保持时长：先按比例重采样改音高（同时改时长），再线性插值拉回原时长，
    净效果只改音高、不改语速。用于在单一 piper 模型上模拟男/女/成人/孩童音色。无 numpy 则原样返回。"""
    if abs(semitones) < 0.01:
        return raw_bytes
    try:
        import numpy as np
    except ImportError:
        return raw_bytes
    ratio = 2.0 ** (semitones / 12.0)
    arr = np.frombuffer(raw_bytes, dtype=np.int16).astype(np.float32)
    n = len(arr)
    if n == 0:
        return raw_bytes
    resampled_len = max(1, int(round(n / ratio)))
    resampled = np.interp(np.linspace(0, n - 1, resampled_len), np.arange(n), arr)
    restored = np.interp(np.linspace(0, resampled_len - 1, n), np.arange(resampled_len), resampled)
    return np.clip(restored, -32768, 32767).astype(np.int16).tobytes()


def main():
    if len(sys.argv) < 3:
        print(json.dumps({"error": "Usage: piper_speak.py <text> <output_wav> [model_path] [pitch_semitones]"}))
        sys.exit(1)

    text = sys.argv[1]
    output_wav = sys.argv[2]
    # 第 3 位 model_path：空串表示用默认（保证 pitch 始终是第 4 位参数）。
    model_path = sys.argv[3] if len(sys.argv) > 3 and sys.argv[3] else None
    try:
        pitch_semitones = float(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] else 0.0
    except ValueError:
        pitch_semitones = 0.0

    script_dir = os.path.dirname(os.path.abspath(__file__))
    if model_path is None:
        model_path = os.path.join(script_dir, "..", "models", "en_US-lessac-medium.onnx")
    elif not os.path.isabs(model_path):
        # web-console 传的是相对路径（如 "models/xxx.onnx"，相对 web-console 目录）；相对脚本位置
        # （tools/..）解析，避免相对进程 cwd（可能是 workspace 根）找不到而回退系统 TTS、丢失音色。
        candidate = os.path.normpath(os.path.join(script_dir, "..", model_path))
        if os.path.exists(candidate):
            model_path = candidate

    if not os.path.exists(model_path):
        print(json.dumps({"error": f"Model not found: {model_path}"}))
        sys.exit(1)

    start = time.time()

    try:
        from piper.voice import PiperVoice

        voice = PiperVoice.load(model_path)
        sample_rate = voice.config.sample_rate
        with wave.open(output_wav, "wb") as wf:
            wf.setnchannels(1)
            wf.setsampwidth(2)
            wf.setframerate(sample_rate)
            voice.synthesize_wav(text, wf)

        # 音调偏移（半音）：读回 PCM，pitch shift 后写回。0 则跳过（等同原行为）。
        if abs(pitch_semitones) >= 0.01:
            with wave.open(output_wav, "rb") as rf:
                sr = rf.getframerate()
                raw = rf.readframes(rf.getnframes())
            shifted = pitch_shift_preserve_duration(raw, pitch_semitones)
            with wave.open(output_wav, "wb") as wf:
                wf.setnchannels(1)
                wf.setsampwidth(2)
                wf.setframerate(sr)
                wf.writeframes(shifted)

    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)

    elapsed_ms = int((time.time() - start) * 1000)
    output = {
        "audio_path": output_wav,
        "duration_ms": elapsed_ms,
    }
    print(json.dumps(output))


if __name__ == "__main__":
    main()
