export interface AudioRecorderOptions {
  sampleRate?: number; // Target sample rate, default 16000
  bufferSize?: number; // Buffer size for processing
  onAudioChunk?: (chunk: ArrayBuffer, pcm16Base64: string) => void;
  onLevels?: (rms: number, peak: number) => void;
  onError?: (error: Error) => void;
}

export class AudioRecorder {
  private holdOpenUntil = 0;

  /** Garde le micro ouvert (envoi continu) jusqu'a `untilMs` (epoch ms). */
  public holdOpen(untilMs: number): void {
    this.holdOpenUntil = Math.max(this.holdOpenUntil, untilMs);
  }

  /** Referme immediatement (interruption : la lecture est coupee). */
  public releaseHold(): void {
    this.holdOpenUntil = 0;
  }

  private targetSampleRate: number;
  private audioContext: AudioContext | null = null;
  private mediaStream: MediaStream | null = null;
  private sourceNode: MediaStreamAudioSourceNode | null = null;
  private analyserNode: AnalyserNode | null = null;
  private processorNode: ScriptProcessorNode | null = null;
  private isCapturing = false;
  private animFrameId: number | null = null;

  private onAudioChunk?: (chunk: ArrayBuffer, pcm16Base64: string) => void;
  private onLevels?: (rms: number, peak: number) => void;
  private onError?: (error: Error) => void;

  constructor(options: AudioRecorderOptions = {}) {
    this.targetSampleRate = options.sampleRate || 16000;
    this.onAudioChunk = options.onAudioChunk;
    this.onLevels = options.onLevels;
    this.onError = options.onError;
  }

  public async start(): Promise<void> {
    if (this.isCapturing) return;

    try {
      // 1. Request microphone access
      this.mediaStream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });

      // 2. Setup AudioContext
      const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      this.audioContext = new AudioCtx();
      if (this.audioContext.state === 'suspended') {
        await this.audioContext.resume();
      }

      this.sourceNode = this.audioContext.createMediaStreamSource(this.mediaStream);

      // 3. Setup Analyser for real-time local RMS/Peak
      this.analyserNode = this.audioContext.createAnalyser();
      this.analyserNode.fftSize = 256;
      this.analyserNode.smoothingTimeConstant = 0.3;
      this.sourceNode.connect(this.analyserNode);

      // 4. Setup Processor Node (buffer size 4096)
      const bufferSize = 4096;
      this.processorNode = this.audioContext.createScriptProcessor(bufferSize, 1, 1);

      // Optimisation batterie : VAD basee sur l'energie RMS.
      // En silence, les trames audio ne sont pas transmises sur le WebSocket pour eviter
      // de faire tourner continuellement le reseau et le CPU en arriere-plan.
      const ENERGY_THRESHOLD = 0.012;
      const PREROLL_MAX = 3; // ~255 ms de pre-roll a 16 kHz
      const HANGOVER_MAX = 19; // ~1600 ms : doit depasser le silence maximal cote nestord (turn.max_silence_ms 1400 ms), sinon l'enonce n'est jamais cloture
      const prerollBuffers: ArrayBuffer[] = [];
      let inVoice = false;
      let hangoverRemaining = 0;

      this.processorNode.onaudioprocess = (e) => {
        if (!this.isCapturing) return;
        const inputData = e.inputBuffer.getChannelData(0);
        const inputSampleRate = this.audioContext?.sampleRate || 48000;

        // Downsample to target 16kHz
        const downsampled = this.downsampleBuffer(inputData, inputSampleRate, this.targetSampleRate);

        // Convert Float32 to Int16 PCM (Linear 16-bit) and compute RMS
        let sumSquares = 0;
        const pcm16 = new Int16Array(downsampled.length);
        for (let i = 0; i < downsampled.length; i++) {
          const s = Math.max(-1, Math.min(1, downsampled[i]));
          sumSquares += s * s;
          pcm16[i] = s < 0 ? s * 0x8000 : s * 0x7fff;
        }
        const rms = Math.sqrt(sumSquares / (downsampled.length || 1));

        if (this.onAudioChunk) {
          const buffer = pcm16.buffer;
          // Le base64 n'est calcule que pour les trames reellement envoyees.
          const send = (buf: ArrayBuffer) => this.onAudioChunk?.(buf, this.arrayBufferToBase64(buf));

          // Pendant la lecture de Nestor, le micro est envoye en continu (sans filtre
          // d'energie) : le daemon en a besoin, avec son signal de reference, pour
          // annuler l'echo de sa propre voix.
          if (rms >= ENERGY_THRESHOLD || Date.now() < this.holdOpenUntil) {
            if (!inVoice) {
              inVoice = true;
              // Vide le pre-roll pour capturer l'attaque du mot
              while (prerollBuffers.length > 0) {
                const pre = prerollBuffers.shift();
                if (pre) send(pre);
              }
            }
            hangoverRemaining = HANGOVER_MAX;
            send(buffer);
          } else if (inVoice) {
            if (hangoverRemaining > 0) {
              hangoverRemaining--;
              send(buffer);
            } else {
              inVoice = false;
              if (prerollBuffers.length >= PREROLL_MAX) prerollBuffers.shift();
              prerollBuffers.push(buffer);
            }
          } else {
            // Silence : economie reseau et CPU
            if (prerollBuffers.length >= PREROLL_MAX) prerollBuffers.shift();
            prerollBuffers.push(buffer);
          }
        }
      };

      this.sourceNode.connect(this.processorNode);
      // ScriptProcessor needs to be connected to destination to trigger events in some browsers
      this.processorNode.connect(this.audioContext.destination);

      this.isCapturing = true;
      this.startLevelsLoop();
    } catch (err) {
      console.error('[AudioRecorder] Failed to start microphone capture:', err);
      if (this.onError) {
        this.onError(err instanceof Error ? err : new Error(String(err)));
      }
      this.stop();
      throw err;
    }
  }

  public stop(): void {
    this.isCapturing = false;

    if (this.animFrameId !== null) {
      cancelAnimationFrame(this.animFrameId);
      this.animFrameId = null;
    }

    if (this.processorNode) {
      this.processorNode.disconnect();
      this.processorNode = null;
    }

    if (this.sourceNode) {
      this.sourceNode.disconnect();
      this.sourceNode = null;
    }

    if (this.analyserNode) {
      this.analyserNode.disconnect();
      this.analyserNode = null;
    }

    if (this.mediaStream) {
      this.mediaStream.getTracks().forEach((t) => t.stop());
      this.mediaStream = null;
    }

    if (this.audioContext && this.audioContext.state !== 'closed') {
      this.audioContext.close().catch(() => {});
      this.audioContext = null;
    }

    if (this.onLevels) {
      this.onLevels(0, 0);
    }
  }

  public isActive(): boolean {
    return this.isCapturing;
  }

  private startLevelsLoop(): void {
    if (!this.analyserNode) return;
    const timeData = new Uint8Array(this.analyserNode.frequencyBinCount);

    const loop = () => {
      if (!this.isCapturing || !this.analyserNode) return;

      this.analyserNode.getByteTimeDomainData(timeData);

      let sumSquares = 0;
      let peak = 0;

      for (let i = 0; i < timeData.length; i++) {
        // Normaliser [0, 255] -> [-1, 1]
        const normalized = (timeData[i] - 128) / 128;
        const absVal = Math.abs(normalized);
        if (absVal > peak) peak = absVal;
        sumSquares += normalized * normalized;
      }

      const rms = Math.sqrt(sumSquares / timeData.length);

      if (this.onLevels) {
        this.onLevels(rms, peak);
      }

      this.animFrameId = requestAnimationFrame(loop);
    };

    this.animFrameId = requestAnimationFrame(loop);
  }

  // Downsample helper: simple linear interpolation
  private downsampleBuffer(buffer: Float32Array, inputRate: number, outputRate: number): Float32Array {
    if (inputRate === outputRate) {
      return buffer;
    }
    const sampleRateRatio = inputRate / outputRate;
    const newLength = Math.round(buffer.length / sampleRateRatio);
    const result = new Float32Array(newLength);
    let offsetResult = 0;
    let offsetBuffer = 0;

    while (offsetResult < result.length) {
      const nextOffsetBuffer = Math.round((offsetResult + 1) * sampleRateRatio);
      let accum = 0;
      let count = 0;
      for (let i = offsetBuffer; i < nextOffsetBuffer && i < buffer.length; i++) {
        accum += buffer[i];
        count++;
      }
      result[offsetResult] = count > 0 ? accum / count : 0;
      offsetResult++;
      offsetBuffer = nextOffsetBuffer;
    }
    return result;
  }

  private arrayBufferToBase64(buffer: ArrayBuffer): string {
    let binary = '';
    const bytes = new Uint8Array(buffer);
    const len = bytes.byteLength;
    for (let i = 0; i < len; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return window.btoa(binary);
  }
}
