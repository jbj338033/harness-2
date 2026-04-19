#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone)]
pub struct ScreenFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub trait Capture: Send + Sync {
    fn capture(&self) -> Result<ScreenFrame, String>;
}

pub trait Pointer: Send + Sync {
    fn move_to(&self, x: i32, y: i32) -> Result<(), String>;
    fn click(&self, button: MouseButton) -> Result<(), String>;
    fn double_click(&self, button: MouseButton) -> Result<(), String>;
    fn button_down(&self, button: MouseButton) -> Result<(), String>;
    fn button_up(&self, button: MouseButton) -> Result<(), String>;
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String>;
}

pub trait Keyboard: Send + Sync {
    fn type_text(&self, text: &str) -> Result<(), String>;
    fn press_key(&self, name: &str) -> Result<(), String>;
}
