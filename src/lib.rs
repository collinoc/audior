use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::SupportedStreamConfig;
use std::sync::mpsc;
use hound::WavSpec;
use std::error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

#[macro_export]
macro_rules! fail {
    ( $msg:expr ) => {{
        eprintln!("Error: {}", $msg);
        std::process::exit(1);
    }};

    ( $msg:expr, $err:expr ) => {{
        eprintln!("Error: {}\n{}", $msg, $err);
        std::process::exit(1);
    }};
}

pub trait WavExt {
    fn as_wav_spec(&self) -> WavSpec;
}

impl WavExt for SupportedStreamConfig {
    fn as_wav_spec(&self) -> WavSpec {
        WavSpec {
            channels: self.channels(),
            sample_rate: self.sample_rate().0,
            bits_per_sample: self.sample_format().sample_size() as u16 * 8,
            sample_format: if self.sample_format().is_float() {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Device {
    Input,
    Output,
}

pub struct DeviceBuilder {
    kind: Device,
    inner: cpal::Device,
    config: SupportedStreamConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    DefaultInputDeviceError,
    DefaultOutputDeviceError,
    DefaultConfigError,
    StreamConfigFormatError,
    StreamCreationError,
    OutputLockError,
    WriterCreationError(String), // TODO: Try to hold the actual error instead of string
    PlayError,
}

impl error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DefaultInputDeviceError => f.write_str("Error getting default input device"),
            Error::DefaultOutputDeviceError => f.write_str("Error getting default output device"),
            Error::DefaultConfigError => f.write_str("Error getting output lock"),
            Error::StreamConfigFormatError => f.write_str("Bad stream config format"),
            Error::StreamCreationError => f.write_str("Error creating stream"),
            Error::OutputLockError => f.write_str("Error getting default device config"),
            Error::WriterCreationError(e) => {
                f.write_fmt(format_args!("Error creating data writer\n{e}"))
            }
            Error::PlayError => f.write_str("Error recording data"),
        }
    }
}

impl DeviceBuilder {
    pub fn new_default_input() -> Result<DeviceBuilder, Error> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(Error::DefaultInputDeviceError)?;

        let config = device
            .default_input_config()
            .or(Err(Error::DefaultConfigError))?;

        Ok(DeviceBuilder {
            kind: Device::Input,
            inner: device,
            config,
        })
    }

    pub fn new_default_output() -> Result<DeviceBuilder, Error> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(Error::DefaultOutputDeviceError)?;

        let config = device
            .default_output_config()
            .or(Err(Error::DefaultConfigError))?;

        Ok(DeviceBuilder {
            kind: Device::Output,
            inner: device,
            config,
        })
    }

    pub fn kind(&self) -> Device {
        self.kind
    }

    pub fn name(&self) -> Result<String, cpal::DeviceNameError> {
        self.inner.name()
    }

    pub fn config(&self) -> &SupportedStreamConfig {
        &self.config
    }

    pub fn use_config(&mut self, config: SupportedStreamConfig) -> &mut Self {
        self.config = config;
        self
    }
}

pub struct StreamBuilder<'a> {
    device: DeviceBuilder,
    config: SupportedStreamConfig,
    stream: Option<cpal::Stream>,
    sender: Option<mpsc::Sender<&'a [u8]>>,
    receiver: Option<mpsc::Receiver<&'a [u8]>>,
    kind: Device,
}

type WavWriter = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

impl<'a> StreamBuilder<'a> {
    pub fn from(device: DeviceBuilder) -> Result<StreamBuilder<'a>, Error> {
        let kind = device.kind;

        let config = match device.kind {
            Device::Input => device
                .inner
                .default_input_config()
                .or(Err(Error::DefaultConfigError))?,
            Device::Output => device
                .inner
                .default_output_config()
                .or(Err(Error::DefaultConfigError))?,
        };

        Ok(StreamBuilder {
            device,
            config,
            stream: None,
            sender: None,
            receiver: None,
            kind,
        })
    }

    pub fn as_input(&mut self) -> &mut Self {
        self.kind = Device::Input;
        self
    }

    pub fn as_output(&mut self) -> &mut Self {
        self.kind = Device::Output;
        self
    }

    pub fn stream(&self) -> Option<&cpal::Stream> {
        self.stream.as_ref()
    }

    pub fn write_to(&mut self, other: DeviceBuilder) -> Result<(), Error> {
        match other.kind {
            Device::Input => {
                todo!()
            }
            Device::Output => {
                let (tx, rx) = mpsc::channel();

                self.sender = Some(tx);
                self.receiver = Some(rx);

                self.build_input_stream(move |bytes: &[f32]| {

                })
                .or(Err(Error::StreamCreationError))?;

                // FIXME: match on type
                self.build_output_stream(move |bytes: &mut [f32]| {
                    if let Some(rx) = self.receiver {
                        let size = std::mem::size_of::<f32>();

                        while let Ok(recv_bytes) = rx.recv() {
                            for (i, slice) in recv_bytes.chunks_exact(size).enumerate() {
                                bytes[i] = 0f32;
                            }
                        }
                    }
                })
                .or(Err(Error::StreamCreationError))?;

            }
        }

        Ok(())
    }

    pub fn write_wav<P>(&mut self, path: P) -> Result<WavWriter, Error>
    where
        P: AsRef<Path>,
    {
        let writer = hound::WavWriter::create(path, self.device.config().as_wav_spec())
            .or_else(|e| Err(Error::WriterCreationError(e.to_string())))?;

        let writer = Arc::new(Mutex::new(Some(writer)));

        let wav_writer = Arc::clone(&writer);

        let stream =
            match self.kind {
                Device::Input => match self.config.sample_format() {
                    cpal::SampleFormat::F32 => self
                        .build_input_stream::<f32>(move |data| write_wav_data(data, &wav_writer)),
                    cpal::SampleFormat::I32 => self
                        .build_input_stream::<i32>(move |data| write_wav_data(data, &wav_writer)),
                    cpal::SampleFormat::I16 => self
                        .build_input_stream::<i16>(move |data| write_wav_data(data, &wav_writer)),
                    cpal::SampleFormat::I8 => {
                        self.build_input_stream::<i8>(move |data| write_wav_data(data, &wav_writer))
                    }
                    _ => return Err(Error::StreamConfigFormatError),
                },
                Device::Output => match self.config.sample_format() {
                    cpal::SampleFormat::F32 => self
                        .build_output_stream::<f32>(move |data| write_wav_data(data, &wav_writer)),
                    cpal::SampleFormat::I32 => self
                        .build_output_stream::<i32>(move |data| write_wav_data(data, &wav_writer)),
                    cpal::SampleFormat::I16 => self
                        .build_output_stream::<i16>(move |data| write_wav_data(data, &wav_writer)),
                    cpal::SampleFormat::I8 => self
                        .build_output_stream::<i8>(move |data| write_wav_data(data, &wav_writer)),
                    _ => return Err(Error::StreamConfigFormatError),
                },
                _ => todo!(),
            }
            .or(Err(Error::StreamCreationError))?;

        self.stream = Some(stream);

        Ok(writer)
    }

    fn build_input_stream<T>(
        &self,
        func: impl Fn(&[T]) + Send + 'static,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample + hound::Sample,
    {
        self.device.inner.build_input_stream(
            &self.config.clone().into(), // TODO: Try to remove clone
            move |data: &[T], _| func(data),
            move |err| fail!("writing data to buffer failed", err),
            None,
        )
    }

    fn build_output_stream<T>(
        &self,
        func: impl Fn(&mut [T]) + Send + 'static,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample + hound::Sample,
    {
        self.device.inner.build_output_stream(
            &self.config.clone().into(), // TODO: Try to remove clone
            move |data: &mut [T], _| func(data),
            move |err| fail!("writing data to buffer failed", err),
            None,
        )
    }

    pub fn play(&self) -> Result<(), Error> {
        if let Some(stream) = &self.stream {
            stream.play().or(Err(Error::PlayError))?;
        }

        Ok(())
    }
}

fn write_wav_data<T>(data: &[T], writer: &WavWriter)
where
    T: cpal::FromSample<T> + cpal::Sample + hound::Sample,
{
    if let Ok(mut wlock) = writer.lock() {
        if let Some(writer) = wlock.as_mut() {
            for &d in data {
                writer
                    .write_sample(T::from_sample(d))
                    .expect("failed to write sample");
            }
        }
    }
}
