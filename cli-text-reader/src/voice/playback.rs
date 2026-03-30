use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::{
  VoicePlayingInfo, chunk_paragraphs, elevenlabs::ElevenLabsService,
};

pub enum PlaybackCommand {
  Start { text: String, doc_start_line: usize, doc_end_line: usize },
  Pause,
  Resume,
  Stop,
}

#[derive(Clone, PartialEq)]
pub enum PlaybackStatus {
  Idle,
  Loading,
  Playing,
  Paused,
}

pub struct PlaybackController {
  cmd_tx: Sender<PlaybackCommand>,
  pub status: Arc<Mutex<PlaybackStatus>>,
  pub voice_error: Arc<Mutex<Option<String>>>,
  pub playing_info: Arc<Mutex<Option<VoicePlayingInfo>>>,
}

impl PlaybackController {
  pub fn new(api_key: String, voice_id: String) -> Self {
    let (cmd_tx, cmd_rx) = mpsc::channel::<PlaybackCommand>();
    let status = Arc::new(Mutex::new(PlaybackStatus::Idle));
    let voice_error = Arc::new(Mutex::new(None::<String>));
    let playing_info = Arc::new(Mutex::new(None::<VoicePlayingInfo>));

    let status_clone = Arc::clone(&status);
    let error_clone = Arc::clone(&voice_error);
    let info_clone = Arc::clone(&playing_info);

    thread::spawn(move || {
      playback_loop(
        api_key,
        voice_id,
        cmd_rx,
        status_clone,
        error_clone,
        info_clone,
      );
    });

    Self { cmd_tx, status, voice_error, playing_info }
  }

  pub fn start(
    &self,
    text: String,
    doc_start_line: usize,
    doc_end_line: usize,
  ) {
    let _ = self.cmd_tx.send(PlaybackCommand::Start {
      text,
      doc_start_line,
      doc_end_line,
    });
  }

  pub fn pause(&self) {
    let _ = self.cmd_tx.send(PlaybackCommand::Pause);
  }

  pub fn resume(&self) {
    let _ = self.cmd_tx.send(PlaybackCommand::Resume);
  }

  pub fn stop(&self) {
    let _ = self.cmd_tx.send(PlaybackCommand::Stop);
  }

  pub fn status(&self) -> PlaybackStatus {
    self.status.lock().unwrap_or_else(|e| e.into_inner()).clone()
  }

  /// Take the pending error message (clears it after reading).
  pub fn take_error(&self) -> Option<String> {
    self.voice_error.lock().unwrap_or_else(|e| e.into_inner()).take()
  }
}

// ---------------------------------------------------------------------------
// Background playback loop
// ---------------------------------------------------------------------------

fn playback_loop(
  api_key: String,
  voice_id: String,
  cmd_rx: Receiver<PlaybackCommand>,
  status: Arc<Mutex<PlaybackStatus>>,
  error: Arc<Mutex<Option<String>>>,
  playing_info: Arc<Mutex<Option<VoicePlayingInfo>>>,
) {
  let service = ElevenLabsService::new(api_key, voice_id);

  let (_stream, handle) = match rodio::OutputStream::try_default() {
    Ok(r) => r,
    Err(e) => {
      *error.lock().unwrap() = Some(format!("Audio init failed: {e}"));
      return;
    }
  };

  for cmd in cmd_rx.iter() {
    match cmd {
      // ------------------------------------------------------------------ //
      PlaybackCommand::Start { text, doc_start_line, doc_end_line } => {
        let sink = match rodio::Sink::try_new(&handle) {
          Ok(s) => s,
          Err(e) => {
            *error.lock().unwrap() = Some(format!("Audio sink error: {e}"));
            continue;
          }
        };
        // Status stays Loading until the first audio chunk is ready to play.
        let mut was_stopped = false;
        let mut chars_before: usize = 0;

        'chunks: for chunk_text in chunk_paragraphs(&text) {
          // Check for interrupt before starting the next network request
          while let Ok(interrupt) = cmd_rx.try_recv() {
            match interrupt {
              PlaybackCommand::Stop => {
                was_stopped = true;
                break 'chunks;
              }
              PlaybackCommand::Pause => {
                sink.pause();
                *status.lock().unwrap() = PlaybackStatus::Paused;
              }
              PlaybackCommand::Resume => {
                sink.play();
                *status.lock().unwrap() = PlaybackStatus::Playing;
              }
              PlaybackCommand::Start { .. } => {
                // New start while fetching — treat as stop; outer loop handles
                was_stopped = true;
                break 'chunks;
              }
            }
          }

          // Start streaming this chunk — returns immediately, fills on bg thread
          let buf = match service.stream(&chunk_text) {
            Err(msg) => {
              *error.lock().unwrap() = Some(msg);
              was_stopped = true;
              break 'chunks;
            }
            Ok(b) => b,
          };

          // Wait for enough bytes for the decoder to parse the MP3 header
          const PRE_BUFFER: usize = 16 * 1024; // ~0.5 s at 256 kbps
          loop {
            if buf.buffered_len() >= PRE_BUFFER || buf.is_done() {
              break;
            }
            if let Ok(interrupt) = cmd_rx.try_recv() {
              match interrupt {
                PlaybackCommand::Stop => {
                  was_stopped = true;
                  break 'chunks;
                }
                _ => {}
              }
            }
            thread::sleep(Duration::from_millis(20));
          }

          let chunk_len = chunk_text.len();
          match rodio::Decoder::new(buf) {
            Ok(source) => {
              // Record when this chunk starts playing and how many chars precede it
              *playing_info.lock().unwrap() = Some(VoicePlayingInfo {
                doc_start_line,
                doc_end_line,
                started_at: Instant::now(),
                chars_before_chunk: chars_before,
              });
              // Decoder started — audio now streams into the sink.
              *status.lock().unwrap() = PlaybackStatus::Playing;
              sink.append(source);
            }
            Err(e) => {
              *error.lock().unwrap() = Some(format!("Audio decode error: {e}"));
              was_stopped = true;
              break 'chunks;
            }
          }

          // Poll until rodio finishes playing this chunk, responding to cmds
          while sink.len() > 0 {
            if let Ok(interrupt) = cmd_rx.try_recv() {
              match interrupt {
                PlaybackCommand::Stop => {
                  was_stopped = true;
                  break 'chunks;
                }
                PlaybackCommand::Pause => {
                  sink.pause();
                  *status.lock().unwrap() = PlaybackStatus::Paused;
                  // Block until Resume or Stop arrives
                  'paused: for pcmd in cmd_rx.iter() {
                    match pcmd {
                      PlaybackCommand::Resume => {
                        sink.play();
                        *status.lock().unwrap() = PlaybackStatus::Playing;
                        break 'paused;
                      }
                      PlaybackCommand::Stop => {
                        was_stopped = true;
                        break 'chunks;
                      }
                      PlaybackCommand::Start { .. } => {
                        was_stopped = true;
                        break 'chunks;
                      }
                      PlaybackCommand::Pause => {} // already paused
                    }
                  }
                }
                PlaybackCommand::Resume => {} // already playing
                PlaybackCommand::Start { .. } => {
                  was_stopped = true;
                  break 'chunks;
                }
              }
            }
            thread::sleep(Duration::from_millis(50));
          }

          chars_before += chunk_len;
        }

        let _ = was_stopped; // silence unused warning
        *playing_info.lock().unwrap() = None;
        *status.lock().unwrap() = PlaybackStatus::Idle;
      }

      // ------------------------------------------------------------------ //
      PlaybackCommand::Stop => {
        *playing_info.lock().unwrap() = None;
        *status.lock().unwrap() = PlaybackStatus::Idle;
      }
      PlaybackCommand::Pause | PlaybackCommand::Resume => {
        // No active session — ignore
      }
    }
  }
}
