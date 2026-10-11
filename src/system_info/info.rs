use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct SystemMessage {
    pub cpu_usage: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub gpu_usage: f32,
    pub gpu_frequency_mhz: u32,
    pub cpu_power_watts: f32,
    pub gpu_power_watts: f32,
    pub ane_power_watts: f32,
    pub total_power_watts: f32,
    pub system_power_watts: f32,
    pub cpu_temp_celsius: f32,
    pub gpu_temp_celsius: f32,
    pub fans: Vec<Fan>,
    pub chip_name: String,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Fan {
    pub name: String,
    pub rpm: u32,
    pub max_rpm: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_message_roundtrip() {
        let msg = SystemMessage {
            cpu_usage: 42.5,
            mem_used: 8_000_000_000,
            mem_total: 16_000_000_000,
            ..Default::default()
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SystemMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cpu_usage, msg.cpu_usage);
        assert_eq!(decoded.mem_used, msg.mem_used);
        assert_eq!(decoded.mem_total, msg.mem_total);
        assert_eq!(decoded.gpu_usage, 0.0);
    }

    #[test]
    fn system_message_with_mac_metrics_roundtrip() {
        let msg = SystemMessage {
            cpu_usage: 10.0,
            mem_used: 4_000_000_000,
            mem_total: 16_000_000_000,
            gpu_usage: 75.0,
            gpu_frequency_mhz: 1200,
            cpu_power_watts: 2.5,
            gpu_power_watts: 4.0,
            total_power_watts: 6.5,
            chip_name: "Apple M3 Pro".to_string(),
            fans: vec![Fan {
                name: "fan0".to_string(),
                rpm: 1200,
                max_rpm: Some(4900),
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SystemMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.gpu_usage, 75.0);
        assert_eq!(decoded.gpu_frequency_mhz, 1200);
        assert_eq!(decoded.cpu_power_watts, 2.5);
        assert_eq!(decoded.fans[0].rpm, 1200);
    }

    #[test]
    fn system_message_default_has_no_gpu() {
        let msg = SystemMessage::default();
        assert_eq!(msg.cpu_usage, 0.0);
        assert_eq!(msg.mem_used, 0);
        assert_eq!(msg.mem_total, 0);
        assert!(msg.fans.is_empty());
    }
}
