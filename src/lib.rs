use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::SupportedStreamConfig;
use hound::WavSpec;
use std::error;

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

    pub fn with_config(&mut self, config: SupportedStreamConfig) -> &mut Self {
        self.config = config;
        self
    }
}

pub struct StreamBuilder {
    device: DeviceBuilder,
    config: SupportedStreamConfig,
    stream: Option<cpal::Stream>,
    kind: Device,
}

impl StreamBuilder {
    pub fn from(device: DeviceBuilder) -> Result<StreamBuilder, Error> {
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

    pub fn play(&self) -> Result<(), Error> {
        if let Some(ref stream) = self.stream {
            stream.play().or(Err(Error::PlayError))?;
        }

        Ok(())
    }

    pub fn with_reader(
        &mut self,
        func: impl Fn(&cpal::Data) + Send + 'static,
    ) -> Result<&cpal::Stream, cpal::BuildStreamError> {
        let format = self.config.sample_format();

        let stream = self.device.inner.build_input_stream_raw(
            &self.config.clone().into(),
            format,
            move |data: &cpal::Data, _: &_| func(data),
            move |err| panic!("writing data to buffer failed: {err}"),
            None,
        )?;
        self.stream = Some(stream);
        println!("Set reader");

        Ok(self.stream.as_ref().unwrap())
    }

    pub fn with_writer(
        &mut self,
        func: impl Fn(&mut [u8]) + Send + 'static,
    ) -> Result<&cpal::Stream, cpal::BuildStreamError> {
        let format = self.config.sample_format();

        let stream = self.device.inner.build_output_stream_raw(
            &self.config.clone().into(),
            format,
            move |data: &mut cpal::Data, _| func(data.bytes_mut()),
            move |err| panic!("writing data to buffer failed {err}"),
            None,
        )?;

        self.stream = Some(stream);

        Ok(self.stream.as_ref().unwrap())
    }
}
