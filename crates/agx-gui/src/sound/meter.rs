//! Read the selected microphone while the Sound page is active. Only levels leave this worker.
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

pub(super) struct Meter {
    pub target: Option<String>,
    request: Sender<Option<String>>,
    level: Arc<AtomicI32>,
}

impl Meter {
    pub fn new() -> Self {
        let (request, updates) = mpsc::channel::<Option<String>>();
        let level = Arc::new(AtomicI32::new(-1));
        let reading = level.clone();
        std::thread::spawn(move || {
            let mut capture: Option<Capture> = None;
            let mut target: Option<String> = None;
            loop {
                let update = match (target.is_some(), capture.is_some()) {
                    (_, true) => updates.recv_timeout(Duration::from_millis(70)),
                    (true, false) => updates.recv_timeout(Duration::from_secs(1)),
                    (false, false) => updates.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };
                match update {
                    Ok(source) => {
                        drop(capture.take());
                        reading.store(-1, Ordering::Relaxed);
                        target = source;
                        capture = target.as_deref().and_then(Capture::open);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let Some(stream) = capture.as_mut() else {
                            capture = target.as_deref().and_then(Capture::open);
                            continue;
                        };
                        match stream.peak() {
                            Ok(level) => reading.store(level, Ordering::Relaxed),
                            Err(_) => {
                                reading.store(-1, Ordering::Relaxed);
                                capture = None;
                            }
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        Self {
            target: None,
            request,
            level,
        }
    }

    pub fn update(&mut self, source: Option<&str>) {
        if self.target.as_deref() != source {
            self.level.store(-1, Ordering::Relaxed);
            self.target = source.map(str::to_owned);
            let _ = self.request.send(self.target.clone());
        }
    }

    pub fn level(&self) -> i32 {
        self.level.load(Ordering::Relaxed)
    }
}

struct Capture {
    child: Child,
    samples: ChildStdout,
    previous_byte: Option<u8>,
    last_read: Instant,
    level: i32,
}

impl Capture {
    fn open(source: &str) -> Option<Self> {
        let mut child = Command::new("pacat")
            .args([
                "--record",
                "--raw",
                "--format=s16le",
                "--rate=16000",
                "--channels=1",
                "--latency-msec=60",
                "--stream-name=ANGI input level",
                "--device",
                source,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let samples = child.stdout.take().unwrap();
        let fd = samples.as_raw_fd();
        if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        Some(Self {
            child,
            samples,
            previous_byte: None,
            last_read: Instant::now(),
            level: -1,
        })
    }

    fn peak(&mut self) -> io::Result<i32> {
        let mut buffer = [0; 4096];
        let mut peak = 0_u16;
        let mut received = false;
        loop {
            let length = match self.samples.read(&mut buffer) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(length) => length,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            };
            received = true;
            let mut data = &buffer[..length];
            if let Some(low) = self.previous_byte.take() {
                peak = i16::from_le_bytes([low, data[0]]).unsigned_abs().max(peak);
                data = &data[1..];
            }
            let chunks = data.chunks_exact(2);
            self.previous_byte = chunks.remainder().first().copied();
            for sample in chunks {
                peak = peak.max(i16::from_le_bytes([sample[0], sample[1]]).unsigned_abs());
            }
        }
        if received {
            // A decibel scale makes quiet speech visible while leaving room for loud input.
            let amplitude = f32::from(peak).max(1.0) / 32768.0;
            self.level = ((1.0 + amplitude.log10() / 3.0).clamp(0.0, 1.0) * 100.0) as i32;
            self.last_read = Instant::now();
        } else if self.last_read.elapsed() > Duration::from_secs(2) {
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(self.level)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
