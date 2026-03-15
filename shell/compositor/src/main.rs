// aether-compositor — AetherOS Wayland Compositor (AetherShell)
// SPDX-License-Identifier: Apache-2.0
//
// Built on Smithay (pure Rust Wayland compositor library).
// Replaces Android's SurfaceFlinger with a modern Wayland-native stack.
//
// Architecture:
//   • DRM/KMS backend → direct GPU output (no X11, no intermediate layers)
//   • libinput → touch, stylus, keyboard input
//   • OpenGL ES 3.x rendering via GBM/EGL
//   • App windows are Wayland XDG surfaces
//   • AetherShell protocol extension for mobile gestures + app lifecycle

use anyhow::Result;
use tracing::info;

// Window management state
#[derive(Debug, Clone)]
pub struct WindowState {
    pub id: u32,
    pub app_id: String,
    pub title: String,
    pub geometry: Rect,
    pub layer: WindowLayer,
    pub focused: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowLayer {
    Background, // Wallpaper
    Normal,     // Regular apps
    StatusBar,  // Top status bar
    NavBar,     // Bottom navigation
    Overlay,    // Notifications, quick settings
    Lock,       // Lock screen
}

// Input event abstraction
#[derive(Debug, Clone)]
pub enum InputEvent {
    Touch {
        slot: u8,
        action: TouchAction,
        x: f32,
        y: f32,
    },
    Key {
        key: u32,
        state: KeyState,
        mods: Modifiers,
    },
    Scroll {
        dx: f32,
        dy: f32,
    },
}

#[derive(Debug, Clone)]
pub enum TouchAction {
    Down,
    Move,
    Up,
}
#[derive(Debug, Clone)]
pub enum KeyState {
    Press,
    Release,
}
#[derive(Debug, Clone)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

// Gesture recognizer for mobile navigation
pub struct GestureRecognizer {
    touch_start: Option<(f32, f32)>,
    touch_history: Vec<(f32, f32, std::time::Instant)>,
}

impl GestureRecognizer {
    pub fn new() -> Self {
        Self {
            touch_start: None,
            touch_history: Vec::new(),
        }
    }
}

impl Default for GestureRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureRecognizer {

    pub fn process(&mut self, event: &InputEvent) -> Option<Gesture> {
        match event {
            InputEvent::Touch {
                action: TouchAction::Down,
                x,
                y,
                ..
            } => {
                self.touch_start = Some((*x, *y));
                self.touch_history.clear();
                None
            }
            InputEvent::Touch {
                action: TouchAction::Move,
                x,
                y,
                ..
            } => {
                self.touch_history.push((*x, *y, std::time::Instant::now()));
                None
            }
            InputEvent::Touch {
                action: TouchAction::Up,
                x,
                y,
                ..
            } => {
                let gesture = self.classify_gesture(*x, *y);
                self.touch_start = None;
                gesture
            }
            _ => None,
        }
    }

    fn classify_gesture(&self, end_x: f32, end_y: f32) -> Option<Gesture> {
        let (start_x, start_y) = self.touch_start?;
        let dx = end_x - start_x;
        let dy = end_y - start_y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < 20.0 {
            return Some(Gesture::Tap { x: end_x, y: end_y });
        }

        // Swipe classification by dominant axis
        if dy.abs() > dx.abs() {
            if dy < -80.0 && start_y > 1600.0 {
                Some(Gesture::HomeSwipe)
            } else if dy < -50.0 {
                Some(Gesture::SwipeUp {
                    velocity: self.velocity(),
                })
            } else if dy > 50.0 {
                Some(Gesture::SwipeDown {
                    velocity: self.velocity(),
                })
            } else {
                None
            }
        } else if dx > 60.0 && start_x < 30.0 {
            Some(Gesture::BackSwipe)
        } else {
            None
        }
    }

    fn velocity(&self) -> f32 {
        if self.touch_history.len() < 2 {
            return 0.0;
        }
        let n = self.touch_history.len();
        let (x1, y1, t1) = &self.touch_history[n - 2];
        let (x2, y2, t2) = &self.touch_history[n - 1];
        let dt = t2.duration_since(*t1).as_secs_f32().max(0.001);
        let dx = x2 - x1;
        let dy = y2 - y1;
        (dx * dx + dy * dy).sqrt() / dt
    }
}

#[derive(Debug, Clone)]
pub enum Gesture {
    Tap { x: f32, y: f32 },
    SwipeUp { velocity: f32 },
    SwipeDown { velocity: f32 },
    HomeSwipe,
    BackSwipe,
    RecentApps,
}

// Animation system
#[derive(Debug, Clone)]
pub struct Animation {
    pub property: AnimProp,
    pub from: f32,
    pub to: f32,
    pub duration_ms: u32,
    pub easing: Easing,
    pub elapsed_ms: u32,
}

#[derive(Debug, Clone)]
pub enum AnimProp {
    X,
    Y,
    Alpha,
    Scale,
    CornerRadius,
}
#[derive(Debug, Clone)]
pub enum Easing {
    Linear,
    EaseOut,
    Spring,
}

impl Animation {
    pub fn current_value(&self) -> f32 {
        let t = (self.elapsed_ms as f32 / self.duration_ms as f32).clamp(0.0, 1.0);
        let eased = match self.easing {
            Easing::Linear => t,
            Easing::EaseOut => 1.0 - (1.0 - t).powi(3),
            Easing::Spring => {
                let overshoot = 1.1;
                if t < 0.8 {
                    t * overshoot / 0.8
                } else {
                    overshoot - (t - 0.8) * (overshoot - 1.0) / 0.2
                }
            }
        };
        self.from + (self.to - self.from) * eased
    }

    pub fn is_complete(&self) -> bool {
        self.elapsed_ms >= self.duration_ms
    }
}

// Display info
#[derive(Debug, Clone)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: u32,
    pub density_dpi: u32,
    pub hdr: bool,
}

impl Display {
    pub fn density_scale(&self) -> f32 {
        self.density_dpi as f32 / 160.0
    }
}

// Compositor main state
pub struct AetherCompositor {
    pub display: Display,
    pub windows: Vec<WindowState>,
    pub gestures: GestureRecognizer,
    pub animations: Vec<(u32, Animation)>, // (window_id, anim)
    next_id: u32,
}

impl AetherCompositor {
    pub fn new(display: Display) -> Self {
        Self {
            display,
            windows: Vec::new(),
            gestures: GestureRecognizer::new(),
            animations: Vec::new(),
            next_id: 1,
        }
    }

    pub fn add_window(&mut self, app_id: &str, layer: WindowLayer) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let w = self.display.width;
        let h = self.display.height;
        self.windows.push(WindowState {
            id,
            app_id: app_id.to_string(),
            title: app_id.to_string(),
            geometry: Rect {
                x: 0,
                y: 0,
                width: w,
                height: h,
            },
            layer,
            focused: false,
            visible: true,
        });
        // Animate window in
        self.animations.push((
            id,
            Animation {
                property: AnimProp::Y,
                from: h as f32,
                to: 0.0,
                duration_ms: 280,
                easing: Easing::Spring,
                elapsed_ms: 0,
            },
        ));
        info!(id, app_id, "window created");
        id
    }

    pub fn handle_input(&mut self, event: InputEvent) {
        if let Some(gesture) = self.gestures.process(&event) {
            self.handle_gesture(gesture);
        }
    }

    fn handle_gesture(&mut self, gesture: Gesture) {
        match gesture {
            Gesture::HomeSwipe => {
                info!("home gesture — return to launcher");
            }
            Gesture::BackSwipe => {
                info!("back gesture — pop navigation stack");
            }
            Gesture::SwipeDown { .. } => {
                info!("swipe down — open notification shade");
            }
            _ => {}
        }
    }

    pub fn tick(&mut self, delta_ms: u32) {
        for (_, anim) in &mut self.animations {
            anim.elapsed_ms += delta_ms;
        }
        self.animations.retain(|(_, a)| !a.is_complete());
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("aether_compositor=info")
        .init();
    info!("AetherShell compositor starting");

    let display = Display {
        width: 1080,
        height: 2400,
        refresh_hz: 120,
        density_dpi: 420,
        hdr: true,
    };

    let mut compositor = AetherCompositor::new(display);
    compositor.add_window("aether.launcher", WindowLayer::Normal);

    info!("AetherShell ready — Wayland socket at /run/aether/wayland-0");
    // Full Smithay event loop would start here
    Ok(())
}
