use voxmux_core::AudioError;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host};

pub struct DeviceManager {
    host: Host,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            host: cpal::default_host(),
        }
    }

    pub fn list_input_devices(&self) -> Result<Vec<(String, Device)>, AudioError> {
        let devices = self
            .host
            .input_devices()
            .map_err(|e| AudioError::DeviceEnumeration(e.to_string()))?;

        let mut result = Vec::new();
        for device in devices {
            let name = device
                .name()
                .unwrap_or_else(|_| "unknown".to_string());
            result.push((name, device));
        }
        Ok(result)
    }

    pub fn list_output_devices(&self) -> Result<Vec<(String, Device)>, AudioError> {
        let devices = self
            .host
            .output_devices()
            .map_err(|e| AudioError::DeviceEnumeration(e.to_string()))?;

        let mut result = Vec::new();
        for device in devices {
            let name = device
                .name()
                .unwrap_or_else(|_| "unknown".to_string());
            result.push((name, device));
        }
        Ok(result)
    }

    pub fn get_input_device(&self, name: &str) -> Result<Device, AudioError> {
        if name == "default" {
            return self
                .host
                .default_input_device()
                .ok_or_else(|| AudioError::DeviceNotFound("no default input device".to_string()));
        }

        let devices = self.list_input_devices()?;
        for (dev_name, device) in devices {
            if dev_name == name {
                return Ok(device);
            }
        }
        Err(AudioError::DeviceNotFound(format!(
            "input device not found: {}",
            name
        )))
    }

    pub fn get_output_device(&self, name: &str) -> Result<Device, AudioError> {
        if name == "default" {
            return self
                .host
                .default_output_device()
                .ok_or_else(|| AudioError::DeviceNotFound("no default output device".to_string()));
        }

        let devices = self.list_output_devices()?;
        for (dev_name, device) in devices {
            if dev_name == name {
                return Ok(device);
            }
        }
        Err(AudioError::DeviceNotFound(format!(
            "output device not found: {}",
            name
        )))
    }
}
