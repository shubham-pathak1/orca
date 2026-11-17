//! Bounded, latest-only Windows media artwork; no file I/O or image decoding on the UI thread.
use crate::jobs::{latest_channel, LatestJob};
use std::{
    io::Cursor,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
};
use windows::{
    Storage::Streams::{DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};
type Completion = (u64, Result<RandomAccessStreamReference, String>);
pub(super) struct CoverWorker {
    requests: LatestJob<(u64, Vec<String>)>,
    result: Arc<Mutex<Option<Completion>>>,
    wanted: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl CoverWorker {
    pub(super) fn new() -> Self {
        let (requests, receiver) = latest_channel::<(u64, Vec<String>)>();
        let result = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let wanted = Arc::new(AtomicU64::new(0));
        let (output, cancel, latest) = (result.clone(), stop.clone(), wanted.clone());
        let thread = thread::spawn(move || {
            let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
            while !cancel.load(Ordering::Relaxed) {
                let Some((generation, paths)) = receiver.receive(&cancel) else {
                    break;
                };
                if latest.load(Ordering::Relaxed) != generation {
                    continue;
                }
                let prepared = if let Err(error) = &initialized {
                    Err(error.to_string())
                } else {
                    prepare_png(&paths, &cancel)
                        .and_then(|png| stream_reference(&png).map_err(|error| error.to_string()))
                };
                if !cancel.load(Ordering::Relaxed) && latest.load(Ordering::Relaxed) == generation {
                    *output.lock().unwrap() = Some((generation, prepared));
                }
            }
            if initialized.is_ok() {
                unsafe { RoUninitialize() };
            }
        });
        Self {
            requests,
            result,
            wanted,
            stop,
            thread: Some(thread),
        }
    }
    pub(super) fn request(&self, generation: u64, paths: Vec<String>) {
        self.wanted.store(generation, Ordering::Relaxed);
        *self.result.lock().unwrap() = None;
        if !paths.is_empty() {
            self.requests.submit((generation, paths));
        }
    }
    pub(super) fn take(&self) -> Option<Completion> {
        self.result.lock().unwrap().take()
    }
}
impl Drop for CoverWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.requests.wake();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn stream_reference(png: &[u8]) -> windows::core::Result<RandomAccessStreamReference> {
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream)?;
    writer.WriteBytes(png)?;
    writer.StoreAsync()?.get()?;
    writer.DetachStream()?;
    stream.Seek(0)?;
    RandomAccessStreamReference::CreateFromStream(&stream)
}
fn prepare_png(paths: &[String], stop: &AtomicBool) -> Result<Vec<u8>, String> {
    for path in paths {
        if stop.load(Ordering::Relaxed) {
            return Err("Media cover cancelled".into());
        }
        let path = Path::new(path);
        if !std::fs::metadata(path)
            .is_ok_and(|file| file.is_file() && file.len() <= 32 * 1024 * 1024)
        {
            continue;
        }
        let Ok(reader) =
            image::ImageReader::open(path).and_then(|reader| reader.with_guessed_format())
        else {
            continue;
        };
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(64 * 1024 * 1024);
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        let mut reader = reader;
        reader.limits(limits);
        let Ok(image) = reader.decode() else { continue };
        if stop.load(Ordering::Relaxed) {
            return Err("Media cover cancelled".into());
        }
        let mut png = Cursor::new(Vec::new());
        image
            .thumbnail(512, 512)
            .to_rgb8()
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        return Ok(png.into_inner());
    }
    Err("No readable media-control cover was found".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_cover_uses_fallback_paths_and_bounded_png_without_touching_source() {
        let folder = std::env::temp_dir().join(format!("orca-media-cover-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("cover.webp");
        image::RgbImage::from_pixel(900, 600, image::Rgb([120, 50, 200]))
            .save(&source)
            .unwrap();
        let before = std::fs::read(&source).unwrap();
        let paths = vec![
            folder.join("missing.png").to_string_lossy().into_owned(),
            source.to_string_lossy().into_owned(),
        ];
        let png = prepare_png(&paths, &AtomicBool::new(false)).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (512, 341));
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(std::fs::read(&source).unwrap(), before);
        assert!(prepare_png(&paths, &AtomicBool::new(true)).is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn latest_media_cover_is_readable_through_windows_and_worker_shuts_down() {
        use std::time::{Duration, Instant};
        use windows::Storage::Streams::DataReader;
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.unwrap();
        let folder = std::env::temp_dir().join(format!("orca-media-stream-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("current.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([12, 34, 56]))
            .save(&source)
            .unwrap();
        let worker = CoverWorker::new();
        worker.request(
            1,
            vec![folder.join("missing.png").to_string_lossy().into_owned()],
        );
        worker.request(2, vec![source.to_string_lossy().into_owned()]);
        let deadline = Instant::now() + Duration::from_secs(4);
        let reference = loop {
            assert!(
                Instant::now() < deadline,
                "media cover worker did not complete"
            );
            if let Some((generation, result)) = worker.take() {
                assert_eq!(generation, 2);
                break result.unwrap();
            }
            thread::sleep(Duration::from_millis(10));
        };
        let stream = reference.OpenReadAsync().unwrap().get().unwrap();
        let size = stream.Size().unwrap() as usize;
        let reader = DataReader::CreateDataReader(&stream).unwrap();
        reader.LoadAsync(size as u32).unwrap().get().unwrap();
        let mut bytes = vec![0; size];
        reader.ReadBytes(&mut bytes).unwrap();
        let pixels = image::load_from_memory(&bytes).unwrap().to_rgb8();
        assert_eq!(pixels.get_pixel(0, 0).0, [12, 34, 56]);
        drop(reader);
        drop(stream);
        drop(reference);
        drop(worker);
        unsafe { RoUninitialize() };
        std::fs::remove_dir_all(folder).unwrap();
    }
}
