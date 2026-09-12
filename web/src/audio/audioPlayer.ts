export interface AudioPlayerOptions {
  onPlaybackLevels?: (rms: number, peak: number) => void;
  onPlaybackStateChange?: (isPlaying: boolean) => void;
}

interface QueuedAudioChunk {
  id: string;
  data: ArrayBuffer;
  format: 'wav' | 'pcm16' | 'pcm';
  sampleRate: number;
}

export class AudioPlayer {
  private audioContext: AudioContext | null = null;
  private analyserNode: AnalyserNode | null = null;
  private currentSourceNode: AudioBufferSourceNode | null = null;
  private queue: QueuedAudioChunk[] = [];
  private isPlaying = false;
  private animFrameId: number | null = null;

  private onPlaybackLevels?: (rms: number, peak: number) => void;
  private onPlaybackStateChange?: (isPlaying: boolean) => void;

  constructor(options: AudioPlayerOptions = {}) {
    this.onPlaybackLevels = options.onPlaybackLevels;
    this.onPlaybackStateChange = options.onPlaybackStateChange;
  }

  private initContext(): AudioContext {
    if (!this.audioContext || this.audioContext.state === 'closed') {
      const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      this.audioContext = new AudioCtx();
    }
    if (!this.analyserNode && this.audioContext) {
      this.analyserNode = this.audioContext.createAnalyser();
      this.analyserNode.fftSize = 256;
      this.analyserNode.smoothingTimeConstant = 0.25;
      this.analyserNode.connect(this.audioContext.destination);
    }
    return this.audioContext;
  }

  public enqueueChunk(base64Data: string, format: 'wav' | 'pcm16' | 'pcm' = 'wav', sampleRate = 24000): void {
    try {
      const binaryString = window.atob(base64Data);
      const len = binaryString.length;
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }

      this.queue.push({
        id: `chunk-${Date.now()}-${Math.random().toString(36).substring(2, 6)}`,
        data: bytes.buffer,
        format,
        sampleRate,
      });

      if (!this.isPlaying) {
        this.playNext();
      }
    } catch (err) {
      console.error('[AudioPlayer] Failed to decode base64 chunk:', err);
    }
  }

  private async playNext(): Promise<void> {
    if (this.queue.length === 0) {
      this.isPlaying = false;
      this.stopLevelsLoop();
      if (this.onPlaybackStateChange) this.onPlaybackStateChange(false);
      return;
    }

    const chunk = this.queue.shift();
    if (!chunk) return;

    this.isPlaying = true;
    if (this.onPlaybackStateChange) this.onPlaybackStateChange(true);

    try {
      const ctx = this.initContext();
      if (ctx.state === 'suspended') {
        await ctx.resume();
      }

      let audioBuffer: AudioBuffer;

      if (chunk.format === 'wav') {
        // Standard WAV audio decoding
        // Note: decodeAudioData detaches the buffer, so pass a slice if needed
        audioBuffer = await ctx.decodeAudioData(chunk.data.slice(0));
      } else {
        // Raw PCM16 (e.g. Kokoro raw 24kHz mono)
        const int16Array = new Int16Array(chunk.data);
        const float32Array = new Float32Array(int16Array.length);
        for (let i = 0; i < int16Array.length; i++) {
          float32Array[i] = int16Array[i] / (int16Array[i] < 0 ? 0x8000 : 0x7fff);
        }
        audioBuffer = ctx.createBuffer(1, float32Array.length, chunk.sampleRate);
        audioBuffer.getChannelData(0).set(float32Array);
      }

      const sourceNode = ctx.createBufferSource();
      sourceNode.buffer = audioBuffer;

      if (this.analyserNode) {
        sourceNode.connect(this.analyserNode);
      } else {
        sourceNode.connect(ctx.destination);
      }

      this.currentSourceNode = sourceNode;
      this.startLevelsLoop();

      sourceNode.onended = () => {
        if (this.currentSourceNode === sourceNode) {
          this.currentSourceNode = null;
          this.playNext();
        }
      };

      sourceNode.start();
    } catch (err) {
      console.error('[AudioPlayer] Playback error on chunk:', err);
      this.currentSourceNode = null;
      this.playNext();
    }
  }

  /**
   * Stop immediately (Barge-In)
   */
  public stop(): void {
    this.queue = [];

    if (this.currentSourceNode) {
      try {
        this.currentSourceNode.stop();
        this.currentSourceNode.disconnect();
      } catch {
        // Ignored if already stopped
      }
      this.currentSourceNode = null;
    }

    // Also cancel browser speech synthesis if active
    if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
      window.speechSynthesis.cancel();
    }

    this.isPlaying = false;
    this.stopLevelsLoop();
    if (this.onPlaybackStateChange) this.onPlaybackStateChange(false);
  }

  public speakBrowserFallback(text: string): void {
    if (typeof window === 'undefined' || !('speechSynthesis' in window)) return;

    this.stop(); // Stop any pending audio
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = 'fr-FR';
    utterance.rate = 1.05;

    // Try to find a French voice
    const voices = window.speechSynthesis.getVoices();
    const frVoice = voices.find((v) => v.lang.startsWith('fr'));
    if (frVoice) {
      utterance.voice = frVoice;
    }

    this.isPlaying = true;
    if (this.onPlaybackStateChange) this.onPlaybackStateChange(true);

    utterance.onend = () => {
      this.isPlaying = false;
      if (this.onPlaybackStateChange) this.onPlaybackStateChange(false);
      if (this.onPlaybackLevels) this.onPlaybackLevels(0, 0);
    };

    utterance.onerror = () => {
      this.isPlaying = false;
      if (this.onPlaybackStateChange) this.onPlaybackStateChange(false);
      if (this.onPlaybackLevels) this.onPlaybackLevels(0, 0);
    };

    window.speechSynthesis.speak(utterance);
  }

  private startLevelsLoop(): void {
    if (!this.analyserNode || this.animFrameId !== null) return;
    const timeData = new Uint8Array(this.analyserNode.frequencyBinCount);

    const loop = () => {
      if (!this.isPlaying || !this.analyserNode) {
        this.stopLevelsLoop();
        return;
      }

      this.analyserNode.getByteTimeDomainData(timeData);

      let sumSquares = 0;
      let peak = 0;

      for (let i = 0; i < timeData.length; i++) {
        const normalized = (timeData[i] - 128) / 128;
        const absVal = Math.abs(normalized);
        if (absVal > peak) peak = absVal;
        sumSquares += normalized * normalized;
      }

      const rms = Math.sqrt(sumSquares / timeData.length);

      if (this.onPlaybackLevels) {
        this.onPlaybackLevels(rms, peak);
      }

      this.animFrameId = requestAnimationFrame(loop);
    };

    this.animFrameId = requestAnimationFrame(loop);
  }

  private stopLevelsLoop(): void {
    if (this.animFrameId !== null) {
      cancelAnimationFrame(this.animFrameId);
      this.animFrameId = null;
    }
    if (this.onPlaybackLevels) {
      this.onPlaybackLevels(0, 0);
    }
  }
}
