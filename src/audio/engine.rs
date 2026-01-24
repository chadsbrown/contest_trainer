use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};

use super::mixer::Mixer;
use crate::config::AudioSettings;
use crate::messages::{AudioCommand, AudioEvent};

pub struct AudioEngine {
    mixer: Arc<Mutex<Mixer>>,
    cmd_rx: Receiver<AudioCommand>,
    _stream: cpal::Stream,
}

impl AudioEngine {
    pub fn new(
        cmd_rx: Receiver<AudioCommand>,
        event_tx: Sender<AudioEvent>,
        settings: AudioSettings,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;

        let supported_config = device.default_output_config()?;
        let sample_rate = supported_config.sample_rate().0;

        // Update settings with actual sample rate
        let mut settings = settings;
        settings.sample_rate = sample_rate;

        let mixer = Arc::new(Mutex::new(Mixer::new(sample_rate, settings.clone())));
        let mixer_for_callback = Arc::clone(&mixer);
        let event_tx_for_callback = event_tx.clone();

        let stream = match supported_config.sample_format() {
            cpal::SampleFormat::F32 => Self::build_stream::<f32>(
                &device,
                &supported_config.into(),
                mixer_for_callback,
                event_tx_for_callback,
            )?,
            cpal::SampleFormat::I16 => Self::build_stream::<i16>(
                &device,
                &supported_config.into(),
                mixer_for_callback,
                event_tx_for_callback,
            )?,
            cpal::SampleFormat::U16 => Self::build_stream::<u16>(
                &device,
                &supported_config.into(),
                mixer_for_callback,
                event_tx_for_callback,
            )?,
            _ => return Err("Unsupported sample format".into()),
        };

        stream.play()?;

        Ok(Self {
            mixer,
            cmd_rx,
            _stream: stream,
        })
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        mixer: Arc<Mutex<Mixer>>,
        event_tx: Sender<AudioEvent>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels as usize;

        device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let num_frames = data.len() / channels;

                let (completed_stations, user_completed) = if channels == 2 {
                    // Stereo output: use stereo mixer for proper L/R separation
                    let mut stereo_buffer = vec![0.0f32; num_frames * 2];

                    let result = {
                        let mut mixer = mixer.lock().unwrap();
                        mixer.fill_stereo_buffer(&mut stereo_buffer)
                    };

                    // Convert interleaved stereo to output format
                    for (frame_idx, frame) in data.chunks_mut(2).enumerate() {
                        let left = stereo_buffer.get(frame_idx * 2).copied().unwrap_or(0.0);
                        let right = stereo_buffer.get(frame_idx * 2 + 1).copied().unwrap_or(0.0);
                        frame[0] = T::from_sample(left);
                        frame[1] = T::from_sample(right);
                    }

                    result
                } else {
                    // Mono or multi-channel: use mono mixer and duplicate
                    let mut mono_buffer = vec![0.0f32; num_frames];

                    let result = {
                        let mut mixer = mixer.lock().unwrap();
                        mixer.fill_buffer(&mut mono_buffer)
                    };

                    // Convert to output format (duplicate mono to all channels)
                    for (frame_idx, frame) in data.chunks_mut(channels).enumerate() {
                        let sample = mono_buffer.get(frame_idx).copied().unwrap_or(0.0);
                        let converted: T = T::from_sample(sample);
                        for channel_sample in frame.iter_mut() {
                            *channel_sample = converted;
                        }
                    }

                    result
                };

                // Send completion events
                for station_id in completed_stations {
                    let _ = event_tx.try_send(AudioEvent::StationComplete(station_id));
                }
                if user_completed {
                    let _ = event_tx.try_send(AudioEvent::UserMessageComplete);
                }
            },
            |err| {
                #[cfg(debug_assertions)]
                eprintln!("Audio stream error: {}", err);
                let _ = err;
            },
            None,
        )
    }

    /// Process pending commands (call this from the main thread periodically)
    pub fn process_commands(&self) {
        loop {
            match self.cmd_rx.try_recv() {
                Ok(cmd) => {
                    let mut mixer = self.mixer.lock().unwrap();
                    match cmd {
                        AudioCommand::StartStation(params) => {
                            // Generate the message the station will send (their callsign)
                            let message = params.callsign.clone();
                            mixer.add_station(&params, &message);
                        }
                        AudioCommand::PlayUserMessage { message, wpm } => {
                            mixer.play_user_message(&message, wpm);
                        }
                        AudioCommand::UpdateSettings(settings) => {
                            mixer.update_settings(settings);
                        }
                        AudioCommand::StopAll => {
                            mixer.clear_all();
                        }
                        AudioCommand::UpdateStereoMode {
                            stereo_enabled,
                            focused_radio,
                        } => {
                            mixer.update_stereo_mode(stereo_enabled, focused_radio);
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }
}
