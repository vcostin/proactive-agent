"""
Kokoro TTS server — OpenAI-compatible /v1/audio/speech endpoint.
Our TtsClient POSTs {"model":"kokoro","input":"text","voice":"af_heart"}
and expects raw WAV bytes back.

Install:
    pip install kokoro-onnx soundfile fastapi uvicorn

Run:
    python scripts/kokoro_server.py --port 8083

Build standalone .exe (Windows):
    scripts\\build-kokoro-exe.ps1

Build standalone binary (macOS/Linux):
    pip install pyinstaller
    pyinstaller --onefile scripts/kokoro_server.py --name kokoro-server
    cp dist/kokoro-server binaries/kokoro-server-aarch64-apple-darwin
"""

import argparse
import io
import logging

import numpy as np
import soundfile as sf
import uvicorn
from fastapi import FastAPI, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel

log = logging.getLogger("kokoro-server")
logging.basicConfig(level=logging.INFO, format="[KOKORO] %(message)s")

# Lazy-load the pipeline — importing kokoro takes a few seconds
_pipeline = None

def get_pipeline():
    global _pipeline
    if _pipeline is None:
        from kokoro import KPipeline  # type: ignore
        log.info("Loading Kokoro model (first run may take a moment)…")
        _pipeline = KPipeline(lang_code="a")  # 'a' = American English
        log.info("Kokoro ready.")
    return _pipeline


app = FastAPI(title="kokoro-server")


class SpeechRequest(BaseModel):
    model: str = "kokoro"
    input: str
    voice: str = "af_heart"
    speed: float = 1.0


@app.get("/health")
def health():
    return {"status": "ok"}


@app.post("/v1/audio/speech")
async def text_to_speech(req: SpeechRequest):
    if not req.input.strip():
        raise HTTPException(status_code=400, detail="input is empty")

    pipeline = get_pipeline()

    try:
        chunks = []
        for _, _, audio in pipeline(req.input, voice=req.voice, speed=req.speed):
            chunks.append(audio)
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e)) from e

    if not chunks:
        raise HTTPException(status_code=500, detail="pipeline produced no audio")

    audio = np.concatenate(chunks).astype(np.float32)

    buf = io.BytesIO()
    sf.write(buf, audio, samplerate=24000, format="WAV", subtype="FLOAT")
    buf.seek(0)

    log.info("synthesised %d chars → %.1f s audio", len(req.input), len(audio) / 24000)
    return Response(content=buf.read(), media_type="audio/wav")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8083)
    args = parser.parse_args()
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")
