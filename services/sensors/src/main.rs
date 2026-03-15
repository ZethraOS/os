// aether-sensord — AetherOS Sensor Aggregation Daemon
// SPDX-License-Identifier: Apache-2.0
//
// Reads raw hardware sensor data via the SensorHal trait,
// applies calibration, fusion algorithms, and delivers events
// to subscribers (apps, compositor, AetherAI).
//
// Sensor fusion provides:
//   • Orientation (quaternion) from accel + gyro + magnetometer
//   • Step detection and counting
//   • Gravity vector
//   • Linear acceleration (accel minus gravity)
//   • Rotation vector (game + geomagnetic)

use anyhow::Result;
use serde::{Deserialize, Serialize};

use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::info;

// ─── Fused sensor data ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }
    pub fn normalize(&self) -> Self {
        let m = self.magnitude().max(1e-10);
        Self::new(self.x / m, self.y / m, self.z / m)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quaternion {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Quaternion {
    pub fn identity() -> Self {
        Self {
            w: 1.0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
        }
    }

    pub fn from_axis_angle(axis: &Vec3, angle_rad: f32) -> Self {
        let half = angle_rad / 2.0;
        let s = half.sin();
        let a = axis.normalize();
        Self {
            w: half.cos(),
            x: a.x * s,
            y: a.y * s,
            z: a.z * s,
        }
    }

    pub fn multiply(&self, other: &Quaternion) -> Self {
        Self {
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
        }
    }

    pub fn normalize(&self) -> Self {
        let n = (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z)
            .sqrt()
            .max(1e-10);
        Self {
            w: self.w / n,
            x: self.x / n,
            y: self.y / n,
            z: self.z / n,
        }
    }

    /// Convert to Euler angles (roll, pitch, yaw) in radians
    pub fn to_euler(&self) -> (f32, f32, f32) {
        let roll = (2.0 * (self.w * self.x + self.y * self.z))
            .atan2(1.0 - 2.0 * (self.x * self.x + self.y * self.y));
        let pitch_arg = (2.0 * (self.w * self.y - self.z * self.x)).clamp(-1.0, 1.0);
        let pitch = pitch_arg.asin();
        let yaw = (2.0 * (self.w * self.z + self.x * self.y))
            .atan2(1.0 - 2.0 * (self.y * self.y + self.z * self.z));
        (roll, pitch, yaw)
    }
}

// ─── Complementary filter for orientation ────────────────────────────────

pub struct ComplementaryFilter {
    orientation: Quaternion,
    alpha: f32, // gyro trust weight (typically 0.98)
}

impl ComplementaryFilter {
    pub fn new(alpha: f32) -> Self {
        Self {
            orientation: Quaternion::identity(),
            alpha,
        }
    }

    /// Update orientation given gyro (rad/s) and accelerometer (m/s²), dt in seconds
    pub fn update(&mut self, gyro: &Vec3, accel: &Vec3, dt: f32) -> &Quaternion {
        // Gyro integration (predict)
        let angle = (gyro.x * gyro.x + gyro.y * gyro.y + gyro.z * gyro.z).sqrt() * dt;
        if angle > 1e-6 {
            let axis = Vec3::new(
                gyro.x / (angle / dt),
                gyro.y / (angle / dt),
                gyro.z / (angle / dt),
            );
            let gyro_delta = Quaternion::from_axis_angle(&axis, angle);
            self.orientation = self.orientation.multiply(&gyro_delta).normalize();
        }

        // Accel correction (if not in free fall)
        let g = accel.magnitude();
        if g > 9.0 && g < 10.6 {
            let a_norm = accel.normalize();
            // Gravity direction from current orientation
            let gravity_est = Vec3::new(
                2.0 * (self.orientation.x * self.orientation.z
                    - self.orientation.w * self.orientation.y),
                2.0 * (self.orientation.w * self.orientation.x
                    + self.orientation.y * self.orientation.z),
                self.orientation.w * self.orientation.w
                    - self.orientation.x * self.orientation.x
                    - self.orientation.y * self.orientation.y
                    + self.orientation.z * self.orientation.z,
            );
            let error = Vec3::new(
                a_norm.y * gravity_est.z - a_norm.z * gravity_est.y,
                a_norm.z * gravity_est.x - a_norm.x * gravity_est.z,
                a_norm.x * gravity_est.y - a_norm.y * gravity_est.x,
            );
            // Blend: use alpha to weight gyro vs accel correction
            let correction = Quaternion::from_axis_angle(&error, (1.0 - self.alpha) * dt);
            self.orientation = self.orientation.multiply(&correction).normalize();
        }

        &self.orientation
    }
}

// ─── Step detector ────────────────────────────────────────────────────────

pub struct StepDetector {
    last_mag: f32,
    threshold: f32,
    min_step_interval_ms: u64,
    last_step_ms: u64,
    step_count: u64,
}

impl StepDetector {
    pub fn new() -> Self {
        Self {
            last_mag: 9.81,
            threshold: 10.8,
            min_step_interval_ms: 300,
            last_step_ms: 0,
            step_count: 0,
        }
    }
}

impl Default for StepDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StepDetector {

    pub fn process(&mut self, accel: &Vec3, timestamp_ms: u64) -> Option<u64> {
        let mag = accel.magnitude();
        // Peak detection: rising above threshold after being below it
        if self.last_mag < self.threshold && mag >= self.threshold
            && timestamp_ms - self.last_step_ms > self.min_step_interval_ms {
                self.step_count += 1;
                self.last_step_ms = timestamp_ms;
                self.last_mag = mag;
                return Some(self.step_count);
        }
        self.last_mag = mag;
        None
    }
}

// ─── Sensor events published to subscribers ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorOutput {
    RawAccelerometer(Vec3),
    RawGyroscope(Vec3),
    RawMagnetometer(Vec3),
    Orientation(Quaternion),
    Gravity(Vec3),
    LinearAcceleration(Vec3),
    StepCount(u64),
    ProximityNear(bool),
    AmbientLux(f32),
    BarometerHpa(f32),
    DeviceOrientation(ScreenOrientation),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScreenOrientation {
    Portrait,
    Landscape,
    ReversePortrait,
    ReverseLandscape,
}

// ─── Sensor daemon ────────────────────────────────────────────────────────

pub struct SensorDaemon {
    filter: ComplementaryFilter,
    step_detector: StepDetector,
    publisher: broadcast::Sender<SensorOutput>,
    current_accel: Vec3,
    current_gyro: Vec3,
    current_orientation: ScreenOrientation,
}

impl SensorDaemon {
    pub fn new(publisher: broadcast::Sender<SensorOutput>) -> Self {
        Self {
            filter: ComplementaryFilter::new(0.98),
            step_detector: StepDetector::new(),
            publisher,
            current_accel: Vec3::new(0.0, 0.0, 9.81),
            current_gyro: Vec3::new(0.0, 0.0, 0.0),
            current_orientation: ScreenOrientation::Portrait,
        }
    }

    fn publish(&self, event: SensorOutput) {
        let _ = self.publisher.send(event);
    }

    fn process_accel(&mut self, a: Vec3, ts_ms: u64) {
        // Gravity separation
        let gravity = Vec3::new(a.x * 0.1, a.y * 0.1, a.z * 0.1 + 9.81 * 0.9);
        let linear = Vec3::new(a.x - gravity.x, a.y - gravity.y, a.z - gravity.z);

        self.publish(SensorOutput::RawAccelerometer(a.clone()));
        self.publish(SensorOutput::Gravity(gravity));
        self.publish(SensorOutput::LinearAcceleration(linear));

        // Step detection
        if let Some(count) = self.step_detector.process(&a, ts_ms) {
            self.publish(SensorOutput::StepCount(count));
        }

        // Screen orientation from gravity vector
        let orientation = self.classify_orientation(&a);
        if orientation != self.current_orientation {
            self.current_orientation = orientation.clone();
            self.publish(SensorOutput::DeviceOrientation(orientation));
        }

        self.current_accel = a;
    }

    fn classify_orientation(&self, accel: &Vec3) -> ScreenOrientation {
        let threshold = 7.0_f32;
        if accel.y.abs() > threshold {
            if accel.y > 0.0 {
                ScreenOrientation::ReversePortrait
            } else {
                ScreenOrientation::Portrait
            }
        } else if accel.x.abs() > threshold {
            if accel.x > 0.0 {
                ScreenOrientation::Landscape
            } else {
                ScreenOrientation::ReverseLandscape
            }
        } else {
            self.current_orientation.clone()
        }
    }

    fn process_gyro(&mut self, g: Vec3, dt: f32) {
        self.publish(SensorOutput::RawGyroscope(g.clone()));
        let orientation = self.filter.update(&g, &self.current_accel, dt).clone();
        self.publish(SensorOutput::Orientation(orientation));
        self.current_gyro = g;
    }

    pub async fn run(mut self) -> Result<()> {
        info!("AetherOS sensor daemon starting");

        // Simulate 100Hz sensor loop (real impl reads from kernel IIO)
        let mut ticker = interval(Duration::from_millis(10));
        let mut t: f32 = 0.0;

        loop {
            ticker.tick().await;
            t += 0.01;

            // In production: read from /sys/bus/iio/devices/iio:device0
            // Simulated values here for development/QEMU
            let accel = Vec3::new(
                (t * 0.5).sin() * 0.5,
                (t * 0.3).cos() * 0.3,
                9.81 + (t * 2.0).sin() * 0.1,
            );
            let gyro = Vec3::new((t * 0.7).sin() * 0.01, (t * 0.4).cos() * 0.01, 0.0);

            self.process_accel(accel, (t * 1000.0) as u64);
            self.process_gyro(gyro, 0.01);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("aether_sensord=info")
        .init();

    let (tx, _) = broadcast::channel(256);
    SensorDaemon::new(tx).run().await
}
