import { useCallback, useEffect, useRef, useState } from "react";

type SpeechRecCtor = new () => SpeechRecognitionLike;

interface SpeechRecognitionLike extends EventTarget {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  start: () => void;
  stop: () => void;
  abort: () => void;
  onresult: ((ev: SpeechRecognitionResultEvent) => void) | null;
  onerror: ((ev: SpeechRecognitionErrorEvent) => void) | null;
  onend: (() => void) | null;
}

interface SpeechRecognitionResultEvent {
  resultIndex: number;
  results: SpeechRecognitionResultList;
}

interface SpeechRecognitionErrorEvent {
  error: string;
  message?: string;
}

function getSpeechRecognitionCtor(): SpeechRecCtor | null {
  if (typeof window === "undefined") return null;
  const w = window as unknown as {
    SpeechRecognition?: SpeechRecCtor;
    webkitSpeechRecognition?: SpeechRecCtor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

interface ChatMediaInteractionProps {
  /** 将识别到的文字合并进输入框 */
  appendTranscript: (text: string) => void;
  /** 禁用媒体按钮（例如正在发送消息时） */
  disabled?: boolean;
}

export function ChatMediaInteraction({
  appendTranscript,
  disabled = false,
}: ChatMediaInteractionProps) {
  const [videoOn, setVideoOn] = useState(false);
  const [audioOn, setAudioOn] = useState(false);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [audioLevel, setAudioLevel] = useState(0);
  const [listening, setListening] = useState(false);
  const [sttHint, setSttHint] = useState<string | null>(null);

  const streamRef = useRef<MediaStream | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const rafRef = useRef<number | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);

  /** 停止轨道与音频节点，不触发 React 状态更新（供 effect 清理使用） */
  const releaseMediaTracks = useCallback(() => {
    streamRef.current?.getTracks().forEach((t) => t.stop());
    streamRef.current = null;
    if (videoRef.current) {
      videoRef.current.srcObject = null;
    }
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    if (audioCtxRef.current) {
      void audioCtxRef.current.close();
      audioCtxRef.current = null;
    }
    analyserRef.current = null;
  }, []);

  const applyStream = useCallback(
    (stream: MediaStream) => {
      releaseMediaTracks();
      setAudioLevel(0);
      streamRef.current = stream;
      const v = videoRef.current;
      if (v) {
        v.srcObject = stream;
        void v.play().catch(() => {
          /* autoplay policies */
        });
      }
      const track = stream.getAudioTracks()[0];
      if (track && audioOn) {
        const ctx = new AudioContext();
        audioCtxRef.current = ctx;
        const source = ctx.createMediaStreamSource(stream);
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 256;
        source.connect(analyser);
        analyserRef.current = analyser;
        const data = new Uint8Array(analyser.frequencyBinCount);
        const tick = () => {
          if (!analyserRef.current) return;
          analyserRef.current.getByteFrequencyData(data);
          let sum = 0;
          for (let i = 0; i < data.length; i++) sum += data[i];
          const avg = sum / (data.length * 255);
          setAudioLevel(avg);
          rafRef.current = requestAnimationFrame(tick);
        };
        rafRef.current = requestAnimationFrame(tick);
      }
    },
    [audioOn, releaseMediaTracks]
  );

  useEffect(() => {
    if (!videoOn && !audioOn) {
      releaseMediaTracks();
      queueMicrotask(() => {
        setMediaError(null);
        setAudioLevel(0);
      });
      return;
    }

    let cancelled = false;

    void (async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({
          video: videoOn ? { facingMode: "user" } : false,
          audio: audioOn,
        });
        if (cancelled) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        queueMicrotask(() => setMediaError(null));
        applyStream(stream);
      } catch (e) {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : String(e);
        queueMicrotask(() => {
          setMediaError(msg);
          setVideoOn(false);
          setAudioOn(false);
        });
        releaseMediaTracks();
        queueMicrotask(() => setAudioLevel(0));
      }
    })();

    return () => {
      cancelled = true;
      releaseMediaTracks();
      queueMicrotask(() => setAudioLevel(0));
    };
  }, [videoOn, audioOn, applyStream, releaseMediaTracks]);

  const toggleVideo = () => {
    if (disabled) return;
    setVideoOn((v) => !v);
  };

  const toggleAudio = () => {
    if (disabled) return;
    setAudioOn((a) => !a);
  };

  const stopSpeechRecognition = useCallback(() => {
    try {
      recognitionRef.current?.stop();
    } catch {
      /* ignore */
    }
    recognitionRef.current = null;
    setListening(false);
  }, []);

  const startSpeechRecognition = useCallback(() => {
    if (disabled) return;
    const Ctor = getSpeechRecognitionCtor();
    if (!Ctor) {
      setSttHint("当前环境不支持浏览器语音转文字（可改用系统听写或手动输入）");
      return;
    }
    setSttHint(null);
    const rec = new Ctor();
    rec.continuous = true;
    rec.interimResults = true;
    rec.lang = "zh-CN";
    rec.onresult = (ev: SpeechRecognitionResultEvent) => {
      for (let i = ev.resultIndex; i < ev.results.length; i++) {
        const row = ev.results[i];
        if (!row.isFinal) continue;
        const chunk = row[0]?.transcript?.trim();
        if (chunk) appendTranscript(chunk);
      }
    };
    rec.onerror = () => {
      setListening(false);
      recognitionRef.current = null;
    };
    rec.onend = () => {
      setListening(false);
      recognitionRef.current = null;
    };
    recognitionRef.current = rec;
    try {
      rec.start();
      setListening(true);
    } catch {
      setSttHint("无法启动语音识别");
      setListening(false);
      recognitionRef.current = null;
    }
  }, [appendTranscript, disabled]);

  useEffect(() => {
    return () => {
      stopSpeechRecognition();
    };
  }, [stopSpeechRecognition]);

  const showPreview = videoOn || audioOn;

  return (
    <div className="chat-media-interaction">
      {mediaError && (
        <div className="chat-media-error" role="status">
          媒体设备：{mediaError}
        </div>
      )}
      {sttHint && (
        <div className="chat-media-hint" role="status">
          {sttHint}
          <button
            type="button"
            className="chat-media-hint-dismiss"
            onClick={() => setSttHint(null)}
            aria-label="关闭提示"
          >
            ✕
          </button>
        </div>
      )}
      {showPreview && (
        <div className="chat-media-preview">
          {videoOn ? (
            <video
              ref={videoRef}
              className="chat-media-video"
              autoPlay
              playsInline
              muted
            />
          ) : (
            <div className="chat-media-audio-only">
              <span className="chat-media-audio-label">仅麦克风</span>
              <div className="chat-media-meter-wrap" aria-hidden>
                <div
                  className="chat-media-meter-fill"
                  style={{ transform: `scaleX(${0.08 + audioLevel * 0.92})` }}
                />
              </div>
            </div>
          )}
          {videoOn && audioOn && (
            <div className="chat-media-meter-overlay" aria-hidden>
              <div
                className="chat-media-meter-fill"
                style={{ transform: `scaleX(${0.08 + audioLevel * 0.92})` }}
              />
            </div>
          )}
        </div>
      )}
      <div className="chat-media-actions">
        <button
          type="button"
          className={`chat-icon-btn chat-media-toggle ${videoOn ? "is-active" : ""}`}
          title={videoOn ? "关闭摄像头" : "开启摄像头"}
          disabled={disabled}
          onClick={toggleVideo}
        >
          <span aria-hidden>▣</span>
        </button>
        <button
          type="button"
          className={`chat-icon-btn chat-media-toggle ${audioOn ? "is-active" : ""}`}
          title={audioOn ? "关闭麦克风" : "开启麦克风"}
          disabled={disabled}
          onClick={toggleAudio}
        >
          <span aria-hidden>🎤</span>
        </button>
        <button
          type="button"
          className={`chat-icon-btn chat-media-toggle ${listening ? "is-active is-listening" : ""}`}
          title={listening ? "停止语音输入" : "语音转文字（中文）"}
          disabled={disabled}
          onClick={() => {
            if (listening) stopSpeechRecognition();
            else void startSpeechRecognition();
          }}
        >
          <span aria-hidden>🗣</span>
        </button>
      </div>
    </div>
  );
}
