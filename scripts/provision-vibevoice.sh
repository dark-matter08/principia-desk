#!/bin/bash
# Provision the VibeVoice audio engine (optional quality tier for audio-lesson
# mode). Creates a uv-managed venv with mlx-audio inside the app data dir.
# Model weights (~2-4 GB) download from Hugging Face on FIRST render.
set -euo pipefail

DATA_DIR="$HOME/Library/Application Support/com.darkmatter.principia-desk"
VENV="$DATA_DIR/vibevoice-venv"

command -v uv >/dev/null || { echo "uv required: brew install uv"; exit 1; }

echo "Creating venv at $VENV ..."
uv venv --python 3.12 "$VENV"
echo "Installing mlx-audio (Apple Silicon TTS) ..."
uv pip install --python "$VENV/bin/python" mlx-audio

"$VENV/bin/python" -c "import mlx_audio; print('mlx-audio OK')"
echo
echo "Provisioned. The next overnight audio render uses VibeVoice instead of"
echo "system speech. First render downloads model weights (be patient)."
