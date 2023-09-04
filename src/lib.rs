use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::SupportedStreamConfig;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Error {
    DefaultInputDeviceError,
    DefaultOutputDeviceError,
    DefaultConfigError,
    StreamConfigFormatError,
    StreamCreationError,
    OutputLockError,
    WriteError,
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
            Error::WriteError => f.write_str("Error writing data"),
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

pub struct StreamBuilder {
    device: DeviceBuilder,
    config: SupportedStreamConfig,
    stream: Option<cpal::Stream>,
    writer: Option<WavWriter>,
    from_kind: Device,
}

type WavWriter = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

impl StreamBuilder {
    pub fn new(device: DeviceBuilder) -> Result<StreamBuilder, Error> {
        let from_kind = device.kind;

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
            writer: None,
            from_kind,
        })
    }

    pub fn from_input(&mut self) -> &mut Self {
        self.from_kind = Device::Input;
        self
    }

    pub fn from_output(&mut self) -> &mut Self {
        self.from_kind = Device::Output;
        self
    }

    pub fn write_wav<P>(&mut self, path: P) -> Result<WavWriter, Error>
    where
        P: AsRef<Path>,
    {
        let writer = hound::WavWriter::create(path, self.device.config().as_wav_spec())
            .or(Err(Error::WriteError))?;
        let writer = Arc::new(Mutex::new(Some(writer)));

        self.writer = Some(Arc::clone(&writer));

        let cfg = self.config.clone(); // TODO: Try to remove this clone
        let wav_writer = Arc::clone(&writer);

        let stream = match self.config.sample_format() {
            cpal::SampleFormat::F32 if self.from_kind == Device::Input => {
                make_wav_input_stream::<f32>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::F32 if self.from_kind == Device::Output => {
                make_wav_output_stream::<f32>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::I32 if self.from_kind == Device::Input => {
                make_wav_input_stream::<i32>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::I32 if self.from_kind == Device::Output => {
                make_wav_output_stream::<i32>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::I16 if self.from_kind == Device::Input => {
                make_wav_input_stream::<i16>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::I16 if self.from_kind == Device::Output => {
                make_wav_output_stream::<i16>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::I8 if self.from_kind == Device::Input => {
                make_wav_input_stream::<i8>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            cpal::SampleFormat::I8 if self.from_kind == Device::Output => {
                make_wav_output_stream::<i8>(&self.device.inner, &cfg.into(), wav_writer)
                    .or(Err(Error::StreamCreationError))?
            }
            _ => return Err(Error::StreamConfigFormatError),
        };

        self.stream = Some(stream);

        Ok(writer)
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
            for &d in data.iter() {
                writer
                    .write_sample(T::from_sample(d))
                    .unwrap_or_else(|err| fail!("failed writing sample", err));
            }
        }
    }
}

fn make_wav_input_stream<T>(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    writer: WavWriter,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample + hound::Sample,
{
    device.build_input_stream(
        cfg,
        move |data: &[T], _| write_wav_data::<T>(data, &writer),
        move |err| fail!("writing data to buffer failed", err),
        None,
    )
}

fn make_wav_output_stream<T>(
    device: &cpal::Device,
    cfg: &cpal::StreamConfig,
    writer: WavWriter,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
T: cpal::SizedSample + hound::Sample,
{
    device.build_output_stream(
        cfg,
        move |data: &mut [T], _| write_wav_data::<T>(data, &writer),
        move |err| fail!("writing data to buffer failed", err),
        None,
    )
}
