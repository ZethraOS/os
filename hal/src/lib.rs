// hal/src/lib.rs — ZethraOS Hardware Abstraction Layer
// SPDX-License-Identifier: Apache-2.0
//
// Defines the trait interfaces that all HAL implementations must satisfy.
// Inspired by Android's Treble HAL model but implemented purely in Rust
// with no HIDL/AIDL dependency — we use async Rust traits over IPC sockets.
//
// Each HAL module:
//   • Lives in its own process (fault isolation)
//   • Communicates via a typed Unix socket protocol
//   • Can be replaced without rebuilding the OS
//   • Is independently updatable

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ─── Camera HAL ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    pub id: String,
    pub facing: CameraFacing,
    pub max_width: u32,
    pub max_height: u32,
    pub max_fps: u32,
    pub has_ois: bool,
    pub has_flash: bool,
    pub focal_lengths_mm: Vec<f32>,
    pub apertures: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CameraFacing { Front, Back, External }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRequest {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub exposure_ns: Option<i64>,   // None = auto
    pub iso: Option<u32>,            // None = auto
    pub af_mode: AfMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageFormat { Yuv420, Jpeg, Raw10, Raw12, Heif }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AfMode { Auto, Continuous, Manual { distance_diopters: f32 } }

#[async_trait::async_trait]
pub trait CameraHal: Send + Sync {
    async fn enumerate_cameras(&self) -> Result<Vec<CameraInfo>>;
    async fn open(&mut self, camera_id: &str) -> Result<()>;
    async fn configure_stream(&mut self, request: &CaptureRequest) -> Result<()>;
    async fn capture_frame(&mut self) -> Result<Vec<u8>>;
    async fn start_preview(&mut self, surface_id: u32) -> Result<()>;
    async fn stop_preview(&mut self) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}

// ─── Sensor HAL ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorEvent {
    pub sensor_type: SensorType,
    pub timestamp_ns: i64,
    pub values: [f32; 6],    // Up to 6 floats; interpretation depends on type
    pub accuracy: SensorAccuracy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SensorType {
    Accelerometer,          // m/s² — [x, y, z]
    Gyroscope,              // rad/s — [x, y, z]
    Magnetometer,           // μT — [x, y, z]
    Proximity,              // cm
    AmbientLight,           // lux
    Barometer,              // hPa
    Thermometer,            // °C
    HeartRate,              // bpm
    StepCounter,            // steps since boot
    GravityVector,          // m/s² — [x, y, z]
    LinearAcceleration,     // m/s² without gravity — [x, y, z]
    RotationVector,         // quaternion [x, y, z, w, accuracy_rad]
    GameRotationVector,     // quaternion [x, y, z, w] (no magnetometer)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorAccuracy { Unreliable, Low, Medium, High }

#[async_trait::async_trait]
pub trait SensorHal: Send + Sync {
    async fn list_sensors(&self) -> Result<Vec<SensorType>>;
    async fn enable(&mut self, sensor: SensorType, sample_rate_hz: f32) -> Result<()>;
    async fn disable(&mut self, sensor: &SensorType) -> Result<()>;
    async fn read_event(&mut self) -> Result<SensorEvent>;
    async fn flush(&mut self, sensor: &SensorType) -> Result<()>;
}

// ─── Display HAL ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub bits_per_pixel: u8,
    pub hdr_type: HdrType,
    pub color_gamut: ColorGamut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HdrType { None, Hdr10, Hdr10Plus, DolbyVision }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColorGamut { Srgb, DisplayP3, BtRec2020 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayAttributes {
    pub brightness: f32,        // 0.0–1.0
    pub auto_brightness: bool,
    pub refresh_rate: u32,
    pub adaptive_refresh: bool, // LPTO-style variable refresh
    pub always_on: bool,
    pub night_mode: bool,
    pub color_temperature_k: u32,
}

#[async_trait::async_trait]
pub trait DisplayHal: Send + Sync {
    async fn get_config(&self) -> Result<DisplayConfig>;
    async fn set_brightness(&mut self, level: f32) -> Result<()>;
    async fn set_refresh_rate(&mut self, hz: u32) -> Result<()>;
    async fn commit_frame(&mut self, framebuffer: &[u8]) -> Result<()>;
    async fn set_hdr_mode(&mut self, mode: HdrType) -> Result<()>;
    async fn set_adaptive_refresh(&mut self, enabled: bool) -> Result<()>;
    async fn blank(&mut self, blank: bool) -> Result<()>; // screen on/off
}

// ─── Biometric HAL ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BiometricResult {
    Authenticated { user_id: u32 },
    Failed { reason: BiometricFailReason },
    Cancelled,
    LockoutTemporary { seconds: u32 },
    LockoutPermanent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BiometricFailReason { NoMatch, FingerNotDetected, SensorDirty, Unknown }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BiometricType { Fingerprint, FaceId, Iris }

#[async_trait::async_trait]
pub trait BiometricHal: Send + Sync {
    async fn get_type(&self) -> Result<BiometricType>;
    async fn enroll_start(&mut self, user_id: u32) -> Result<String>; // returns enrollment token
    async fn enroll_capture(&mut self, token: &str) -> Result<EnrollProgress>;
    async fn authenticate(&mut self, challenge: u64) -> Result<BiometricResult>;
    async fn remove_enrollment(&mut self, user_id: u32) -> Result<()>;
    async fn get_enrolled_ids(&self) -> Result<Vec<u32>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrollProgress {
    pub percent: u8,
    pub complete: bool,
    pub remaining_touches: u8,
    pub quality: u8, // 0–100
}

// ─── Power HAL ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub level_percent: u8,
    pub charging: bool,
    pub charge_type: ChargeType,
    pub temperature_c: f32,
    pub voltage_mv: u32,
    pub current_ma: i32,    // negative = discharging
    pub full_capacity_mah: u32,
    pub design_capacity_mah: u32,
    pub cycle_count: u32,
    pub health: BatteryHealth,
    pub estimated_hours_remaining: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChargeType { None, Wired5W, Wired18W, Wired45W, Wireless5W, Wireless15W }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatteryHealth { Good, Overheat, Dead, Overvoltage, UnspecFailure, Cold }

#[async_trait::async_trait]
pub trait PowerHal: Send + Sync {
    async fn get_battery_info(&self) -> Result<BatteryInfo>;
    async fn set_wakelock(&mut self, name: &str, acquired: bool) -> Result<()>;
    async fn request_performance_mode(&mut self, mode: PerfMode) -> Result<()>;
    async fn schedule_reboot(&mut self, delay_secs: u64) -> Result<()>;
    async fn power_off(&mut self) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerfMode { PowerSave, Balanced, Performance, Gaming }
