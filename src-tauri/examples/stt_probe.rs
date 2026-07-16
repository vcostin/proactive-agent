use cpal::traits::{DeviceTrait, HostTrait};
use proactive_agent_lib::audio::{pick_input_config, quiet_backend_probe_noise, resolve_input_device};
fn main() {
    quiet_backend_probe_noise();
    let host = cpal::default_host();
    let device = resolve_input_device(&host).unwrap();
    let def = device.default_input_config().unwrap();
    let picked = pick_input_config(&device).unwrap();
    println!("default: {} Hz {} ch {:?}", def.sample_rate().0, def.channels(), def.sample_format());
    println!("picked:  {} Hz {} ch {:?}", picked.sample_rate().0, picked.channels(), picked.sample_format());
}
