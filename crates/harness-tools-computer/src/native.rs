use crate::traits::{Capture, Keyboard, MouseButton, Pointer, ScreenFrame};
use std::sync::Mutex;

#[derive(Default)]
pub struct NativeScreen;

impl Capture for NativeScreen {
    fn capture(&self) -> Result<ScreenFrame, String> {
        let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
        let monitor = monitors
            .into_iter()
            .next()
            .ok_or_else(|| "no monitors".to_string())?;
        let image = monitor.capture_image().map_err(|e| e.to_string())?;
        let (w, h) = image.dimensions();
        Ok(ScreenFrame {
            width: w,
            height: h,
            pixels: image.into_raw(),
        })
    }
}

pub struct NativePointer {
    inner: Mutex<enigo::Enigo>,
}

impl NativePointer {
    pub fn try_new() -> Result<Self, String> {
        let enigo = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Mutex::new(enigo),
        })
    }
}

impl Pointer for NativePointer {
    fn move_to(&self, x: i32, y: i32) -> Result<(), String> {
        use enigo::Mouse;
        self.inner
            .lock()
            .unwrap()
            .move_mouse(x, y, enigo::Coordinate::Abs)
            .map_err(|e| e.to_string())
    }
    fn click(&self, button: MouseButton) -> Result<(), String> {
        use enigo::Mouse;
        self.inner
            .lock()
            .unwrap()
            .button(to_enigo(button), enigo::Direction::Click)
            .map_err(|e| e.to_string())
    }
    fn double_click(&self, button: MouseButton) -> Result<(), String> {
        use enigo::Mouse;
        let mut guard = self.inner.lock().expect("mutex poisoned");
        guard
            .button(to_enigo(button), enigo::Direction::Click)
            .map_err(|e| e.to_string())?;
        guard
            .button(to_enigo(button), enigo::Direction::Click)
            .map_err(|e| e.to_string())
    }
    fn button_down(&self, button: MouseButton) -> Result<(), String> {
        use enigo::Mouse;
        self.inner
            .lock()
            .unwrap()
            .button(to_enigo(button), enigo::Direction::Press)
            .map_err(|e| e.to_string())
    }
    fn button_up(&self, button: MouseButton) -> Result<(), String> {
        use enigo::Mouse;
        self.inner
            .lock()
            .unwrap()
            .button(to_enigo(button), enigo::Direction::Release)
            .map_err(|e| e.to_string())
    }
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        use enigo::Mouse;
        let mut g = self.inner.lock().expect("mutex poisoned");
        if dx != 0 {
            g.scroll(dx, enigo::Axis::Horizontal)
                .map_err(|e| e.to_string())?;
        }
        if dy != 0 {
            g.scroll(dy, enigo::Axis::Vertical)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub struct NativeKeyboard {
    inner: Mutex<enigo::Enigo>,
}

impl NativeKeyboard {
    pub fn try_new() -> Result<Self, String> {
        let enigo = enigo::Enigo::new(&enigo::Settings::default()).map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Mutex::new(enigo),
        })
    }
}

impl Keyboard for NativeKeyboard {
    fn type_text(&self, text: &str) -> Result<(), String> {
        use enigo::Keyboard;
        self.inner
            .lock()
            .unwrap()
            .text(text)
            .map_err(|e| e.to_string())
    }
    fn press_key(&self, name: &str) -> Result<(), String> {
        use enigo::Keyboard;
        let key = map_key(name).ok_or_else(|| format!("unknown key: {name}"))?;
        self.inner
            .lock()
            .unwrap()
            .key(key, enigo::Direction::Click)
            .map_err(|e| e.to_string())
    }
}

fn to_enigo(b: MouseButton) -> enigo::Button {
    match b {
        MouseButton::Left => enigo::Button::Left,
        MouseButton::Right => enigo::Button::Right,
        MouseButton::Middle => enigo::Button::Middle,
    }
}

fn map_key(name: &str) -> Option<enigo::Key> {
    use enigo::Key;
    Some(match name {
        "Return" | "Enter" => Key::Return,
        "Escape" | "Esc" => Key::Escape,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Space" => Key::Space,
        "ArrowUp" | "Up" => Key::UpArrow,
        "ArrowDown" | "Down" => Key::DownArrow,
        "ArrowLeft" | "Left" => Key::LeftArrow,
        "ArrowRight" | "Right" => Key::RightArrow,
        s if s.chars().count() == 1 => Key::Unicode(s.chars().next()?),
        _ => return None,
    })
}
