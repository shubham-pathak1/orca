//! Selected Windows endpoints. COM remains on the endpoint worker; decoding
//! feeds a bounded PCM queue from a separate producer.
use super::{
    buffered::{produce, PcmQueue},
    OutputConfig, OutputDevice, OutputStatus,
};
use crate::integer_audio::IntegerFormat;
use rodio::mixer::{mixer, Mixer};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};
static DRIVER_UNRESPONSIVE: AtomicBool = AtomicBool::new(false);

// Driver calls cannot be safely interrupted. Bound the engine's wait and stop
// further native opens if a worker is stuck, rather than leaking one per retry.
fn finish_worker(worker: JoinHandle<()>) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while !worker.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    if worker.is_finished() {
        let _ = worker.join();
    } else {
        DRIVER_UNRESPONSIVE.store(true, Ordering::Release);
    }
}

// COM objects must be created and destroyed on their owning worker thread.
struct Com;
impl Com {
    fn new() -> Result<Self, String> {
        wasapi::initialize_mta().ok().map_err(|e| e.to_string())?;
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        wasapi::deinitialize();
    }
}

pub fn devices() -> Result<Vec<OutputDevice>, String> {
    let _com = Com::new()?;
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let default = enumerator
        .get_default_device(&Direction::Render)
        .and_then(|d| d.get_id())
        .ok();
    let collection = enumerator
        .get_device_collection(&Direction::Render)
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for index in 0..collection.get_nbr_devices().map_err(|e| e.to_string())? {
        let device = collection
            .get_device_at_index(index)
            .map_err(|e| e.to_string())?;
        if let (Ok(id), Ok(name)) = (device.get_id(), device.get_friendlyname()) {
            result.push(OutputDevice {
                is_default: default.as_ref() == Some(&id),
                id,
                name,
            });
        }
    }
    Ok(result)
}

enum Control {
    Active(bool),
    Reset,
    Shutdown,
}
// Rust drops fields in declaration order. The event must outlive both COM
// interfaces; closing it first makes Windows wait for its event timeout.
struct Session {
    render: Option<wasapi::AudioRenderClient>,
    client: AudioClient,
    event: Option<wasapi::Handle>,
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.client.stop_stream();
    }
}
pub struct OutputHandle<T = f32> {
    mixer: Mixer,
    queue: Arc<PcmQueue<T>>,
    control: mpsc::Sender<Control>,
    thread: Option<JoinHandle<()>>,
    active: std::sync::atomic::AtomicBool,
    error: Arc<Mutex<Option<String>>>,
}
impl OutputHandle {
    pub fn open(
        config: &OutputConfig,
        preferred_rate: Option<u32>,
    ) -> Result<(Self, OutputStatus), String> {
        Self::open_stream(
            config,
            preferred_rate,
            None,
            |samples| Box::new(samples),
            encode_sample,
        )
    }
}

impl OutputHandle<i32> {
    /// Exact integer output never falls back to float, a narrower word, a
    /// different rate or a different channel count.
    pub fn open_integer(
        config: &OutputConfig,
        format: IntegerFormat,
        source: impl Iterator<Item = i32> + Send + 'static,
    ) -> Result<(Self, OutputStatus), String> {
        if !config.exclusive {
            return Err("Unprocessed integer output requires exclusive mode".into());
        }
        if !(8000..=768000).contains(&format.rate)
            || !(1..=32).contains(&format.channels)
            || !(1..=32).contains(&format.bits)
        {
            return Err("Invalid integer output format".into());
        }
        Self::open_stream(
            config,
            Some(format.rate),
            Some(format),
            move |_| Box::new(source),
            encode_integer_sample,
        )
    }
}

type Encoder<T> = fn(T, &SampleType, u16, &mut [u8]);
impl<T: Copy + Default + Send + 'static> OutputHandle<T> {
    fn open_stream(
        config: &OutputConfig,
        preferred_rate: Option<u32>,
        integer_format: Option<IntegerFormat>,
        make_source: impl FnOnce(rodio::mixer::MixerSource) -> Box<dyn Iterator<Item = T> + Send>
            + Send
            + 'static,
        encode: Encoder<T>,
    ) -> Result<(Self, OutputStatus), String> {
        if config.device_id.len() > 8192 || config.device_id.contains('\0') {
            return Err("Invalid output device ID".into());
        }
        if DRIVER_UNRESPONSIVE.load(Ordering::Acquire) {
            return Err("A Windows audio driver stopped responding. Restart Orca before retrying selected output.".into());
        }
        let queue = Arc::new(PcmQueue::default());
        let worker_queue = queue.clone();
        let config = config.clone();
        let (control, receive) = mpsc::channel();
        let (ready, startup) = mpsc::sync_channel(1);
        let error = Arc::new(Mutex::new(None));
        let runtime_error = error.clone();
        let thread = thread::spawn(move || {
            // Initialization failures are returned before the engine resumes playback.
            let _com = match Com::new() {
                Ok(com) => com,
                Err(e) => {
                    let _ = ready.send(Err(e));
                    return;
                }
            };
            match render(
                &config,
                preferred_rate,
                integer_format,
                make_source,
                encode,
                &receive,
                &ready,
                &worker_queue,
            ) {
                Ok(()) => (),
                Err(e) => {
                    let _ = ready.try_send(Err(e.clone()));
                    if let Ok(mut slot) = runtime_error.lock() {
                        *slot = Some(e);
                    }
                }
            }
        });
        match startup.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok((mixer, status))) => Ok((
                Self {
                    mixer,
                    queue,
                    control,
                    thread: Some(thread),
                    active: false.into(),
                    error,
                },
                status,
            )),
            Ok(Err(error)) => {
                queue.stop();
                let _ = control.send(Control::Shutdown);
                finish_worker(thread);
                Err(error)
            }
            Err(error) => {
                queue.stop();
                let _ = control.send(Control::Shutdown);
                finish_worker(thread);
                Err(format!(
                    "Windows audio initialization did not complete: {error}"
                ))
            }
        }
    }
    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }
    pub fn has_pending(&self) -> bool {
        self.queue.has_pending()
    }
    pub fn set_draining(&self, draining: bool) {
        self.queue.set_draining(draining);
    }
    pub fn discard_pending(&self) {
        self.active.store(false, Ordering::Relaxed);
        self.queue.discard();
        let _ = self.control.send(Control::Reset);
    }
    pub fn set_active(&self, active: bool) {
        self.queue.set_active(active);
        if self
            .active
            .swap(active, std::sync::atomic::Ordering::Relaxed)
            != active
        {
            let _ = self.control.send(Control::Active(active));
        }
    }
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().ok()?.take()
    }
}
impl<T> Drop for OutputHandle<T> {
    fn drop(&mut self) {
        // Wake a producer parked on its bounded queue before waiting for it.
        // No sample operations are required during shutdown.
        self.queue.stop_worker();
        let _ = self.control.send(Control::Shutdown);
        if let Some(thread) = self.thread.take() {
            finish_worker(thread);
        }
    }
}

fn exclusive_format(
    client: &AudioClient,
    preferred_rate: u32,
    device_rate: u32,
) -> Result<WaveFormat, String> {
    negotiate_exclusive(preferred_rate, device_rate, |format| {
        client.is_supported_exclusive_with_quirks(format).ok()
    })
}

fn negotiate_exclusive(
    preferred_rate: u32,
    device_rate: u32,
    mut supported: impl FnMut(&WaveFormat) -> Option<WaveFormat>,
) -> Result<WaveFormat, String> {
    let mut rates = vec![preferred_rate, device_rate, 48000, 44100, 96000, 192000];
    let mut seen = std::collections::HashSet::new();
    rates.retain(|rate| seen.insert(*rate));
    for rate in rates {
        for (bits, valid, kind) in [
            (32, 32, SampleType::Float),
            (32, 24, SampleType::Int),
            (24, 24, SampleType::Int),
            (16, 16, SampleType::Int),
        ] {
            for channels in [2, 1] {
                let format = WaveFormat::new(bits, valid, &kind, rate as usize, channels, None);
                if let Some(format) = supported(&format) {
                    return Ok(format);
                }
            }
        }
    }
    Err("The device has no supported exclusive PCM format. Use shared output.".into())
}

#[allow(clippy::too_many_arguments)]
fn render<T: Copy + Default + Send + 'static>(
    config: &OutputConfig,
    preferred_rate: Option<u32>,
    integer_format: Option<IntegerFormat>,
    make_source: impl FnOnce(rodio::mixer::MixerSource) -> Box<dyn Iterator<Item = T> + Send>,
    encode: Encoder<T>,
    control: &mpsc::Receiver<Control>,
    ready: &mpsc::SyncSender<Result<(Mixer, OutputStatus), String>>,
    queue: &Arc<PcmQueue<T>>,
) -> Result<(), String> {
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let device = if config.device_id.is_empty() {
        enumerator.get_default_device(&Direction::Render)
    } else {
        enumerator.get_device(&config.device_id)
    }
    .map_err(|e| format!("Cannot open output device: {e}"))?;
    let mut client = device.get_iaudioclient().map_err(|e| e.to_string())?;
    let mix_format = client.get_mixformat().map_err(|e| e.to_string())?;
    let format = if let Some(integer) = integer_format {
        negotiate_integer(integer, |format| {
            client.is_supported_exclusive_with_quirks(format).ok()
        })?
    } else if config.exclusive {
        exclusive_format(
            &client,
            preferred_rate.unwrap_or(mix_format.get_samplespersec()),
            mix_format.get_samplespersec(),
        )?
    } else {
        WaveFormat::new(
            32,
            32,
            &SampleType::Float,
            mix_format.get_samplespersec() as usize,
            mix_format.get_nchannels() as usize,
            None,
        )
    };
    let mode = if config.exclusive {
        let (_, minimum) = client.get_device_period().map_err(|e| e.to_string())?;
        let period = client
            .calculate_aligned_period_near(minimum.max(200_000), Some(128), &format)
            .map_err(|e| e.to_string())?;
        StreamMode::EventsExclusive { period_hns: period }
    } else {
        StreamMode::EventsShared {
            autoconvert: true,
            buffer_duration_hns: 200_000,
        }
    };
    let initialization = client.initialize_client(&format, &Direction::Render, &mode);
    if let Err(error) = initialization {
        // Some HDA drivers return a required buffer alignment after Initialize.
        let alignment_error =
            matches!(&error, wasapi::WasapiError::Windows(e) if e.code().0 as u32 == 0x88890019);
        if !config.exclusive || !alignment_error {
            return Err(format!(
                "Cannot initialize WASAPI {} output: {error}",
                if config.exclusive {
                    "exclusive"
                } else {
                    "shared"
                }
            ));
        }
        let frames = client.get_buffer_size().map_err(|e| e.to_string())?;
        client = device.get_iaudioclient().map_err(|e| e.to_string())?;
        let aligned = StreamMode::EventsExclusive {
            period_hns: wasapi::calculate_period_100ns(
                frames as i64,
                format.get_samplespersec() as i64,
            ),
        };
        client
            .initialize_client(&format, &Direction::Render, &aligned)
            .map_err(|e| e.to_string())?;
    }
    let mut session = Session {
        render: None,
        client,
        event: None,
    };
    session.event = Some(
        session
            .client
            .set_get_eventhandle()
            .map_err(|e| e.to_string())?,
    );
    session.render = Some(
        session
            .client
            .get_audiorenderclient()
            .map_err(|e| e.to_string())?,
    );
    let client = &session.client;
    let event = session.event.as_ref().unwrap();
    let render = session.render.as_ref().unwrap();
    let frames = client.get_buffer_size().map_err(|e| e.to_string())? as usize;
    let channels = format.get_nchannels();
    let bytes_per_sample = format.get_bitspersample() as usize / 8;
    let kind = format.get_subformat().map_err(|e| e.to_string())?;
    validate_pcm(&kind, format.get_validbitspersample(), bytes_per_sample)?;
    if channels == 0
        || channels > 32
        || frames == 0
        || frames > format.get_samplespersec() as usize
        || format.get_blockalign() as usize != channels as usize * bytes_per_sample
    {
        return Err("Invalid PCM buffer layout from audio driver".into());
    }
    let (mixer, samples) = mixer(channels, format.get_samplespersec());
    let status = OutputStatus {
        config: config.clone(),
        device_name: device.get_friendlyname().map_err(|e| e.to_string())?,
        sample_rate: format.get_samplespersec(),
        source_sample_rate: preferred_rate.unwrap_or_default(),
        notice: preferred_rate.filter(|rate| config.exclusive && *rate!=format.get_samplespersec()).map(|rate| format!("The device does not support {rate} Hz in exclusive mode. Using {} Hz with resampling.",format.get_samplespersec())).unwrap_or_default(),
        channels,
        sample_format: format!("{}-bit {:?}", format.get_validbitspersample(), kind),
        ..Default::default()
    };
    ready.send(Ok((mixer, status))).map_err(|e| e.to_string())?;
    let producer = produce(
        make_source(samples),
        queue.clone(),
        (format.get_samplespersec() as usize / 10).max(frames * 2) * channels as usize,
        channels as usize,
    )
    .map_err(|e| e.to_string())?;
    let result = (|| {
        let mut buffer = vec![0u8; frames * format.get_blockalign() as usize];
        let mut active = false;
        let mut timeouts = 0;
        loop {
            let command = if active {
                control.try_recv().ok()
            } else {
                Some(control.recv().map_err(|e| e.to_string())?)
            };
            match command {
                Some(Control::Shutdown) => break,
                Some(Control::Reset) => {
                    if active {
                        client.stop_stream().map_err(|e| e.to_string())?;
                    }
                    client.reset_stream().map_err(|e| e.to_string())?;
                    active = false;
                }
                Some(Control::Active(value)) if value != active => {
                    if value {
                        let available = client
                            .get_available_space_in_frames()
                            .map_err(|e| e.to_string())?
                            as usize;
                        if available > frames {
                            return Err("Invalid WASAPI buffer size".into());
                        }
                        buffer.fill(0);
                        render
                            .write_to_device(
                                available,
                                &buffer[..available * format.get_blockalign() as usize],
                                None,
                            )
                            .map_err(|e| e.to_string())?;
                        client.start_stream().map_err(|e| e.to_string())?;
                    } else {
                        client.stop_stream().map_err(|e| e.to_string())?;
                        client.reset_stream().map_err(|e| e.to_string())?;
                    }
                    active = value;
                }
                _ => (),
            }
            if !active {
                continue;
            }
            if event.wait_for_event(50).is_err() {
                timeouts += 1;
                if timeouts >= 60 {
                    return Err(
                        "WASAPI output stopped responding. Check the device connection.".into(),
                    );
                }
                continue;
            }
            timeouts = 0;
            let available = client
                .get_available_space_in_frames()
                .map_err(|e| e.to_string())? as usize;
            let byte_count = available * format.get_blockalign() as usize;
            if byte_count > buffer.len() {
                return Err("WASAPI returned an invalid buffer size".into());
            }
            let mut chunks = buffer[..byte_count].chunks_exact_mut(bytes_per_sample);
            let consumed = queue.read(
                |sample| {
                    encode(
                        sample,
                        &kind,
                        format.get_validbitspersample(),
                        chunks.next().unwrap(),
                    )
                },
                available * channels as usize,
            );
            if consumed > 0 {
                queue.submitted(Duration::from_secs_f64(
                    frames as f64 / format.get_samplespersec() as f64,
                ));
            }
            if available > 0 {
                render
                    .write_to_device(available, &buffer[..byte_count], None)
                    .map_err(|e| e.to_string())?;
            }
        }
        if active {
            let _ = client.stop_stream();
        }
        Ok(())
    })();
    queue.stop();
    finish_worker(producer);
    result
}

fn validate_pcm(kind: &SampleType, bits: u16, bytes: usize) -> Result<(), String> {
    super::pcm::validate_pcm(*kind == SampleType::Float, bits, bytes)
}

fn encode_sample(sample: f32, kind: &SampleType, valid_bits: u16, output: &mut [u8]) {
    super::pcm::encode_sample(sample, *kind == SampleType::Float, valid_bits, output);
}

fn encode_integer_sample(sample: i32, _: &SampleType, _: u16, output: &mut [u8]) {
    // Negotiation already guarantees enough valid bits. Decoder samples are
    // left-aligned, so copying the high container bytes is lossless.
    crate::integer_audio::write_integer(sample, output);
}

fn negotiate_integer(
    source: IntegerFormat,
    mut supported: impl FnMut(&WaveFormat) -> Option<WaveFormat>,
) -> Result<WaveFormat, String> {
    for (container, valid) in [(16, 16), (32, 24), (24, 24), (32, 32)] {
        if valid < source.bits {
            continue;
        }
        let candidate = WaveFormat::new(
            container,
            valid as usize,
            &SampleType::Int,
            source.rate as usize,
            source.channels as usize,
            None,
        );
        if let Some(actual) = supported(&candidate) {
            if actual.get_samplespersec() == source.rate
                && actual.get_nchannels() == source.channels
                && actual.get_validbitspersample() >= source.bits
                && actual.get_subformat().ok() == Some(SampleType::Int)
            {
                return Ok(actual);
            }
        }
    }
    Err(format!("unsupported-source-format: The device cannot preserve {} Hz, {}-bit, {}-channel integer PCM. Select normal output to allow conversion.", source.rate, source.bits, source.channels))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_integer_negotiation_never_reduces_precision_rate_or_channels() {
        let source = IntegerFormat {
            rate: 192000,
            channels: 2,
            bits: 32,
        };
        let mut attempts = 0;
        let chosen = negotiate_integer(source, |candidate| {
            attempts += 1;
            assert_eq!(candidate.get_validbitspersample(), 32);
            assert_eq!(candidate.get_samplespersec(), 192000);
            Some(candidate.clone())
        })
        .unwrap();
        assert_eq!(attempts, 1);
        assert_eq!(chosen.get_validbitspersample(), 32);
        for bad in [
            WaveFormat::new(32, 24, &SampleType::Int, 192000, 2, None),
            WaveFormat::new(32, 32, &SampleType::Float, 192000, 2, None),
            WaveFormat::new(32, 32, &SampleType::Int, 48000, 2, None),
            WaveFormat::new(32, 32, &SampleType::Int, 192000, 1, None),
        ] {
            assert!(negotiate_integer(source, |_| Some(bad.clone())).is_err());
        }
        for sample in [i32::MIN, i32::MAX, -1, 1, 0x12345679] {
            let mut bytes = [0; 4];
            encode_integer_sample(sample, &SampleType::Int, 32, &mut bytes);
            assert_eq!(bytes, sample.to_le_bytes());
        }
    }
    #[test]
    fn native_rate_is_tried_before_any_fallback_container() {
        let mut probed = Vec::new();
        let chosen = negotiate_exclusive(192000, 48000, |format| {
            probed.push(format.get_samplespersec());
            (format.get_bitspersample() == 16).then(|| format.clone())
        })
        .unwrap();
        assert_eq!(chosen.get_samplespersec(), 192000);
        assert!(probed.iter().all(|rate| *rate == 192000));
    }

    #[test]
    fn unsupported_source_rate_uses_supported_device_rate_or_reports_failure() {
        let chosen = negotiate_exclusive(192000, 48000, |format| {
            (format.get_samplespersec() == 48000).then(|| format.clone())
        })
        .unwrap();
        assert_eq!(chosen.get_samplespersec(), 48000);
        assert!(negotiate_exclusive(192000, 48000, |_| None).is_err());
    }

    #[test]
    fn unsupported_pcm_containers_are_rejected_before_conversion() {
        for (kind, bits, bytes) in [
            (SampleType::Float, 64, 8),
            (SampleType::Float, 24, 4),
            (SampleType::Int, 0, 2),
            (SampleType::Int, 24, 2),
            (SampleType::Int, 64, 8),
        ] {
            assert!(validate_pcm(&kind, bits, bytes).is_err());
        }
        assert!(validate_pcm(&SampleType::Float, 32, 4).is_ok());
    }
    #[test]
    fn pcm_conversion_clips_and_left_aligns_valid_bits() {
        let mut pcm = [0u8; 4];
        encode_sample(1.0, &SampleType::Int, 24, &mut pcm);
        assert_eq!(pcm, [0, 255, 255, 127]);
        encode_sample(-1.0, &SampleType::Int, 24, &mut pcm);
        assert_eq!(pcm, [0, 0, 0, 128]);
        let mut pcm = [0u8; 3];
        encode_sample(-1.0, &SampleType::Int, 24, &mut pcm);
        assert_eq!(pcm, [0, 0, 128]);
        let mut pcm = [0u8; 2];
        encode_sample(f32::NAN, &SampleType::Int, 16, &mut pcm);
        assert_eq!(pcm, [0, 0]);
    }
}
