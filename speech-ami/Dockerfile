FROM python:3.11-slim

# Install system dependencies for audio processing
RUN apt-get update && apt-get install -y \
    ffmpeg \
    git \
    && rm -rf /var/lib/apt/lists/*

# Install Python dependencies
RUN pip install --no-cache-dir \
    boto3==1.34.19 \
    openai-whisper==20231117 \
    torch==2.1.0 \
    torchaudio==2.1.0

WORKDIR /app

# Copy the processing script
COPY process_audio.py /app/

# Run the processor
CMD ["python3", "process_audio.py"]
