//! Speech-to-text (STT) and text-to-speech (TTS) backend seam.
//!
//! "Very small, very fast" local audio: rather than bundling a heavy ML
//! runtime, the default [`CommandAudioBackend`] shells out to tiny local
//! binaries (e.g. `whisper.cpp`, `piper`, `espeak-ng`) you already have,
//! passing arguments as a `Vec<String>` (never a shell string) so model
//! output can't inject commands. [`StubAudioBackend`] returns a clear
//! "not wired" error until a backend is supplied. A future local GGUF runner
//! (Kokoro / whisper) can implement [`AudioBackend`] directly.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio backend not wired: {0}")]
    Unsupported(String),
    #[error("audio execution failed: {0}")]
    Execution(String),
}

/// Text-to-speech request.
pub struct TtsRequest {
    pub text: String,
    pub voice: Option<String>,
    /// Where to write the audio; defaults to `tts_output.wav` in the cwd.
    pub out_path: Option<PathBuf>,
}

/// Text-to-speech result.
pub struct TtsResponse {
    pub out_path: PathBuf,
    pub bytes: usize,
}

/// Speech-to-text request.
pub struct SttRequest {
    pub audio_path: PathBuf,
    pub model: Option<String>,
}

/// Speech-to-text result.
pub struct SttResponse {
    pub text: String,
}

/// A source of local STT/TTS. Implementors must be `Send + Sync` so they can
/// live inside an `Arc<dyn Tool>`.
pub trait AudioBackend: Send + Sync {
    fn tts(&self, req: &TtsRequest) -> Result<TtsResponse, AudioError>;
    fn stt(&self, req: &SttRequest) -> Result<SttResponse, AudioError>;
}

/// Returns [`AudioError::Unsupported`] for everything — used until a real
/// backend is supplied (keeps the framework fully exercisable without audio).
pub struct StubAudioBackend;

impl AudioBackend for StubAudioBackend {
    fn tts(&self, _req: &TtsRequest) -> Result<TtsResponse, AudioError> {
        Err(AudioError::Unsupported(
            "no audio backend configured; supply CommandAudioBackend or a local GGUF runner".into(),
        ))
    }
    fn stt(&self, _req: &SttRequest) -> Result<SttResponse, AudioError> {
        Err(AudioError::Unsupported(
            "no audio backend configured; supply CommandAudioBackend or a local GGUF runner".into(),
        ))
    }
}

/// Shells out to local binaries with argument templates. `tts_args` /
/// `stt_args` are `Vec<String>` templates; placeholders `{text}`, `{voice}`,
/// `{out}`, `{audio}`, `{model}` are substituted. No shell is invoked, so a
/// malicious `text` cannot break out into extra commands.
pub struct CommandAudioBackend {
    pub tts_args: Vec<String>,
    pub stt_args: Vec<String>,
}

impl CommandAudioBackend {
    pub fn new(tts_args: Vec<String>, stt_args: Vec<String>) -> Self {
        Self { tts_args, stt_args }
    }

    fn resolve(template: &[String], view: &RequestView) -> Vec<String> {
        template
            .iter()
            .map(|a| {
                a.replace("{text}", view.text)
                    .replace("{voice}", view.voice)
                    .replace("{out}", view.out)
                    .replace("{audio}", view.audio)
                    .replace("{model}", view.model)
            })
            .collect()
    }
}

struct RequestView<'a> {
    text: &'a str,
    voice: &'a str,
    out: &'a str,
    audio: &'a str,
    model: &'a str,
}

impl AudioBackend for CommandAudioBackend {
    fn tts(&self, req: &TtsRequest) -> Result<TtsResponse, AudioError> {
        if self.tts_args.is_empty() {
            return Err(AudioError::Unsupported("tts command not configured".into()));
        }
        let out = req
            .out_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("tts_output.wav"));
        let view = RequestView {
            text: &req.text,
            voice: req.voice.as_deref().unwrap_or(""),
            out: &out.to_string_lossy(),
            audio: "",
            model: "",
        };
        let args = Self::resolve(&self.tts_args, &view);
        let (program, rest) = args.split_first().expect("tts_args non-empty");
        let status = std::process::Command::new(program)
            .args(rest)
            .status()
            .map_err(|e| AudioError::Execution(format!("failed to spawn {program}: {e}")))?;
        if !status.success() {
            return Err(AudioError::Execution(format!("tts command exited {status}")));
        }
        let bytes = std::fs::metadata(&out).map(|m| m.len() as usize).unwrap_or(0);
        Ok(TtsResponse {
            out_path: out,
            bytes,
        })
    }

    fn stt(&self, req: &SttRequest) -> Result<SttResponse, AudioError> {
        if self.stt_args.is_empty() {
            return Err(AudioError::Unsupported("stt command not configured".into()));
        }
        let view = RequestView {
            text: "",
            voice: "",
            out: "",
            audio: &req.audio_path.to_string_lossy(),
            model: req.model.as_deref().unwrap_or(""),
        };
        let args = Self::resolve(&self.stt_args, &view);
        let (program, rest) = args.split_first().expect("stt_args non-empty");
        let output = std::process::Command::new(program)
            .args(rest)
            .output()
            .map_err(|e| AudioError::Execution(format!("failed to spawn {program}: {e}")))?;
        if !output.status.success() {
            return Err(AudioError::Execution(format!(
                "stt command exited {}",
                output.status
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(SttResponse { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_returns_unsupported() {
        let b = StubAudioBackend;
        assert!(matches!(
            b.tts(&TtsRequest {
                text: "x".into(),
                voice: None,
                out_path: None
            }),
            Err(AudioError::Unsupported(_))
        ));
        assert!(matches!(
            b.stt(&SttRequest {
                audio_path: "a.wav".into(),
                model: None
            }),
            Err(AudioError::Unsupported(_))
        ));
    }

    #[test]
    fn command_backend_passes_args_without_shell_injection() {
        // `echo` is invoked as a literal binary (no shell), so a hostile query
        // stays a single argument and is echoed verbatim — no command breakout.
        let b = CommandAudioBackend::new(vec![], vec!["echo".into(), "{audio}".into()]);
        let resp = b
            .stt(&SttRequest {
                audio_path: "/tmp/foo bar; rm -rf /".into(),
                model: None,
            })
            .unwrap();
        assert_eq!(resp.text, "/tmp/foo bar; rm -rf /");
    }

    #[test]
    fn command_tts_reports_produced_file_size() {
        let dir = std::env::temp_dir().join("roco-audio-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.wav");
        // `sh -c` is used only in-test to materialize a file (no untrusted input).
        let b = CommandAudioBackend::new(
            vec!["sh".into(), "-c".into(), "printf 'RIFF' > {out}".into()],
            vec![],
        );
        let resp = b
            .tts(&TtsRequest {
                text: "hi".into(),
                voice: None,
                out_path: Some(out.clone()),
            })
            .unwrap();
        assert_eq!(resp.out_path, out);
        assert_eq!(resp.bytes, 4);
    }
}
