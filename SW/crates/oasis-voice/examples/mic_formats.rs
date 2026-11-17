//! Print what each microphone says it supports (for diagnosing "format not supported").
use cpal::traits::{DeviceTrait, HostTrait};
fn main() {
    let host = cpal::default_host();
    for device in host.input_devices().unwrap() {
        println!("{}", device.name().unwrap_or_default());
        println!("  default: {:?}", device.default_input_config());
        if let Ok(configs) = device.supported_input_configs() {
            for c in configs {
                println!("  supports: {:?} ch, {:?}-{:?} Hz, {:?}", c.channels(), c.min_sample_rate().0, c.max_sample_rate().0, c.sample_format());
            }
        }
    }
}
