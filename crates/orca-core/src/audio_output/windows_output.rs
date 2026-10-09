use super::{OutputConfig, OutputDevice, OutputStatus};
use rodio::mixer::{mixer, Mixer};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

// COM objects must be created and destroyed on their owning worker thread.
struct Com;
impl Com {
    fn new() -> Result<Self, String> { wasapi::initialize_mta().ok().map_err(|e| e.to_string())?; Ok(Self) }
}
impl Drop for Com { fn drop(&mut self) { wasapi::deinitialize(); } }

pub fn devices() -> Result<Vec<OutputDevice>, String> {
    let _com = Com::new()?;
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let default = enumerator.get_default_device(&Direction::Render).and_then(|d| d.get_id()).ok();
    let collection = enumerator.get_device_collection(&Direction::Render).map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for index in 0..collection.get_nbr_devices().map_err(|e| e.to_string())? {
        let device = collection.get_device_at_index(index).map_err(|e| e.to_string())?;
        if let (Ok(id), Ok(name)) = (device.get_id(), device.get_friendlyname()) {
            result.push(OutputDevice { is_default: default.as_ref() == Some(&id), id, name });
        }
    }
    Ok(result)
}

enum Control { Active(bool), Shutdown }
// Rust drops fields in declaration order. The event must outlive both COM
// interfaces; closing it first makes Windows wait for its event timeout.
struct Session {
    render: Option<wasapi::AudioRenderClient>,
    client: AudioClient,
    event: Option<wasapi::Handle>,
}
impl Drop for Session {
    fn drop(&mut self) { let _ = self.client.stop_stream(); }
}
pub struct OutputHandle {
    mixer: Mixer,
    control: mpsc::Sender<Control>,
    thread: Option<JoinHandle<()>>,
    active: std::sync::atomic::AtomicBool,
    error: Arc<Mutex<Option<String>>>,
}
impl OutputHandle {
    pub fn open(config: &OutputConfig) -> Result<(Self, OutputStatus), String> {
        let config = config.clone();
        let (control, receive) = mpsc::channel();
        let (ready, startup) = mpsc::sync_channel(1);
        let error = Arc::new(Mutex::new(None));
        let runtime_error = error.clone();
        let thread = thread::spawn(move || {
            // Initialization failures are returned before the engine resumes playback.
            eprintln!("render: COM");
            let _com = match Com::new() { Ok(com) => com, Err(e) => { let _ = ready.send(Err(e)); return; } };
            match render(&config, &receive, &ready) {
                Ok(()) => (),
                Err(e) => { eprintln!("render: error {e}"); let _ = ready.try_send(Err(e.clone())); *runtime_error.lock().unwrap() = Some(e); }
            }
        });
        match startup.recv() {
            Ok(Ok((mixer, status))) => Ok((Self { mixer, control, thread: Some(thread),
                active: false.into(), error }, status)),
            Ok(Err(error)) => { let _ = thread.join(); Err(error) }
            Err(error) => { let _ = thread.join(); Err(error.to_string()) }
        }
    }
    pub fn mixer(&self) -> &Mixer { &self.mixer }
    pub fn set_active(&self, active: bool) {
        if self.active.swap(active, std::sync::atomic::Ordering::Relaxed) != active {
            let _ = self.control.send(Control::Active(active));
        }
    }
    pub fn take_error(&self) -> Option<String> { self.error.lock().unwrap().take() }
}
impl Drop for OutputHandle {
    fn drop(&mut self) { let _ = self.control.send(Control::Shutdown); if let Some(thread) = self.thread.take() { let _ = thread.join(); } }
}

fn exclusive_format(client: &AudioClient, preferred_rate: u32) -> Result<WaveFormat, String> {
    for rate in [preferred_rate, 48000, 44100, 96000] {
        for (bits, valid, kind) in [(32,32,SampleType::Float),(32,24,SampleType::Int),(24,24,SampleType::Int),(16,16,SampleType::Int)] {
            for channels in [2,1] {
                let format = WaveFormat::new(bits, valid, &kind, rate as usize, channels, None);
                if let Ok(format) = client.is_supported_exclusive_with_quirks(&format) { return Ok(format); }
            }
        }
    }
    Err("The device has no supported exclusive PCM format. Use shared output.".into())
}

fn render(config: &OutputConfig, control: &mpsc::Receiver<Control>, ready: &mpsc::SyncSender<Result<(Mixer, OutputStatus), String>>) -> Result<(), String> {
    eprintln!("render: enumerate");
    let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
    let device = if config.device_id.is_empty() { enumerator.get_default_device(&Direction::Render) }
        else { enumerator.get_device(&config.device_id) }.map_err(|e| format!("Cannot open output device: {e}"))?;
    let mut client = device.get_iaudioclient().map_err(|e| e.to_string())?;
    let mix_format = client.get_mixformat().map_err(|e| e.to_string())?;
    let format = if config.exclusive { exclusive_format(&client, mix_format.get_samplespersec())? }
        else { mix_format };
    let mode = if config.exclusive {
        let (_, minimum) = client.get_device_period().map_err(|e| e.to_string())?;
        let period = client.calculate_aligned_period_near(minimum.max(200_000),Some(128),&format).map_err(|e| e.to_string())?;
        StreamMode::EventsExclusive { period_hns: period }
    } else { StreamMode::EventsShared { autoconvert: false, buffer_duration_hns: 0 } };
    eprintln!("render: initialize {:?}", mode);
    let mut initialization = client.initialize_client(&format,&Direction::Render,&mode);
    // Windows may keep the previous shared session alive briefly after Release.
    for _ in 0..6 {
        let busy = matches!(&initialization, Err(wasapi::WasapiError::Windows(e)) if e.code().0 as u32 == 0x8889000a);
        if !config.exclusive || !busy { break; }
        std::thread::sleep(std::time::Duration::from_millis(250));
        client = device.get_iaudioclient().map_err(|e| e.to_string())?;
        initialization = client.initialize_client(&format,&Direction::Render,&mode);
    }
    if let Err(error) = initialization {
        // Some HDA drivers return a required buffer alignment after Initialize.
        let alignment_error = matches!(&error, wasapi::WasapiError::Windows(e) if e.code().0 as u32 == 0x88890019);
        if !config.exclusive || !alignment_error { return Err(format!("Cannot initialize WASAPI {} output: {error}",if config.exclusive {"exclusive"} else {"shared"})); }
        let frames = client.get_buffer_size().map_err(|e| e.to_string())?;
        client = device.get_iaudioclient().map_err(|e| e.to_string())?;
        let aligned = StreamMode::EventsExclusive { period_hns: wasapi::calculate_period_100ns(frames as i64,format.get_samplespersec() as i64) };
        client.initialize_client(&format,&Direction::Render,&aligned).map_err(|e| e.to_string())?;
    }
    eprintln!("render: initialized");
    let mut session = Session { render:None,client,event:None };
    session.event = Some(session.client.set_get_eventhandle().map_err(|e| e.to_string())?);
    session.render = Some(session.client.get_audiorenderclient().map_err(|e| e.to_string())?);
    let client = &session.client;
    let event = session.event.as_ref().unwrap();
    let render = session.render.as_ref().unwrap();
    let frames = client.get_buffer_size().map_err(|e| e.to_string())? as usize;
    let channels = format.get_nchannels();
    let bytes_per_sample = format.get_bitspersample() as usize / 8;
    let kind = format.get_subformat().map_err(|e| e.to_string())?;
    let (mixer, mut samples) = mixer(channels,format.get_samplespersec());
    let status = OutputStatus { config: config.clone(), device_name: device.get_friendlyname().map_err(|e| e.to_string())?,
        sample_rate: format.get_samplespersec(),channels,sample_format: format!("{}-bit {:?}",format.get_validbitspersample(),kind),..Default::default() };
    eprintln!("render: ready");
    ready.send(Ok((mixer,status))).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; frames * format.get_blockalign() as usize];
    let mut active = false;
    let mut timeouts = 0;
    loop {
        let command = if active { control.try_recv().ok() } else { Some(control.recv().map_err(|e| e.to_string())?) };
        match command {
            Some(Control::Shutdown) => break,
            Some(Control::Active(value)) if value != active => {
                if value {
                    let available = client.get_available_space_in_frames().map_err(|e| e.to_string())? as usize;
                    buffer.fill(0);
                    render.write_to_device(available,&buffer[..available*format.get_blockalign() as usize],None).map_err(|e| e.to_string())?;
                    client.start_stream().map_err(|e| e.to_string())?;
                } else { client.stop_stream().map_err(|e| e.to_string())?;client.reset_stream().map_err(|e| e.to_string())?; }
                active = value;
            }
            _ => (),
        }
        if !active { continue; }
        if event.wait_for_event(100).is_err() {
            timeouts += 1;if timeouts >= 30 { return Err("WASAPI output stopped responding. Check the device connection.".into()); }
            continue;
        }
        timeouts = 0;
        let available = client.get_available_space_in_frames().map_err(|e| e.to_string())? as usize;
        let byte_count = available*format.get_blockalign() as usize;
        if byte_count > buffer.len() { return Err("WASAPI returned an invalid buffer size".into()); }
        for sample in buffer[..byte_count].chunks_exact_mut(bytes_per_sample) {
            encode_sample(samples.next().unwrap_or(0.0),&kind,format.get_validbitspersample(),sample);
        }
        if available > 0 { render.write_to_device(available,&buffer[..byte_count],None).map_err(|e| e.to_string())?; }
    }
    eprintln!("render: shutdown");
    if active { let _ = client.stop_stream(); }
    eprintln!("render: stopped");
    Ok(())
}

fn encode_sample(sample: f32, kind: &SampleType, valid_bits: u16, output: &mut [u8]) {
    let sample = if sample.is_finite() { sample.clamp(-1.0,1.0) } else { 0.0 };
    if *kind == SampleType::Float { output.copy_from_slice(&sample.to_le_bytes()); return; }
    let scale = (1i64 << (valid_bits-1)) as f64;
    let value = ((sample as f64 * scale).round() as i64).clamp(-(scale as i64),scale as i64-1);
    let padded = (value << (output.len()*8-valid_bits as usize)) as i32;
    output.copy_from_slice(&padded.to_le_bytes()[..output.len()]);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_conversion_clips_and_left_aligns_valid_bits() {
        let mut pcm = [0u8;4];encode_sample(1.0,&SampleType::Int,24,&mut pcm);assert_eq!(pcm,[0,255,255,127]);
        encode_sample(-1.0,&SampleType::Int,24,&mut pcm);assert_eq!(pcm,[0,0,0,128]);
        let mut pcm = [0u8;3];encode_sample(-1.0,&SampleType::Int,24,&mut pcm);assert_eq!(pcm,[0,0,128]);
        let mut pcm = [0u8;2];encode_sample(f32::NAN,&SampleType::Int,16,&mut pcm);assert_eq!(pcm,[0,0]);
    }
}
