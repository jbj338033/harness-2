use crate::traits::{Capture, Keyboard, MouseButton, Pointer, ScreenFrame};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose};
use harness_tools::{Tool, ToolContext, ToolError, ToolOutput};
use image::{ImageBuffer, Rgba};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Cursor;
use std::sync::Arc;

pub struct ComputerTool {
    screen: Arc<dyn Capture>,
    pointer: Arc<dyn Pointer>,
    keyboard: Arc<dyn Keyboard>,
}

impl ComputerTool {
    #[must_use]
    pub fn new(
        screen: Arc<dyn Capture>,
        pointer: Arc<dyn Pointer>,
        keyboard: Arc<dyn Keyboard>,
    ) -> Self {
        Self {
            screen,
            pointer,
            keyboard,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Input {
    Screenshot {
        #[serde(default)]
        region: Option<Region>,
        #[serde(default = "default_quality")]
        jpeg_quality: u8,
    },
    Click {
        x: i32,
        y: i32,
        #[serde(default)]
        button: Option<Button>,
    },
    DoubleClick {
        x: i32,
        y: i32,
        #[serde(default)]
        button: Option<Button>,
    },
    MouseDown {
        x: i32,
        y: i32,
        #[serde(default)]
        button: Option<Button>,
    },
    MouseUp {
        x: i32,
        y: i32,
        #[serde(default)]
        button: Option<Button>,
    },
    Move {
        x: i32,
        y: i32,
    },
    Scroll {
        #[serde(default)]
        dx: i32,
        #[serde(default)]
        dy: i32,
    },
    TypeText {
        text: String,
    },
    Key {
        name: String,
    },
}

#[derive(Deserialize, Clone, Copy)]
struct Region {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Button {
    Left,
    Right,
    Middle,
}

impl From<Option<Button>> for MouseButton {
    fn from(b: Option<Button>) -> Self {
        match b {
            Some(Button::Right) => MouseButton::Right,
            Some(Button::Middle) => MouseButton::Middle,
            _ => MouseButton::Left,
        }
    }
}

fn default_quality() -> u8 {
    75
}

#[async_trait]
impl Tool for ComputerTool {
    fn name(&self) -> &'static str {
        "computer_use"
    }

    fn description(&self) -> &'static str {
        "Capture the screen or drive the mouse / keyboard.\n\
         USE: interact with apps that have no scriptable API.\n\
         DO NOT USE: for web pages (use `browser` — accessibility snapshot is cheaper)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "oneOf": [
                {"properties": {"action": {"const": "screenshot"}}},
                {"required": ["action","x","y"], "properties": {"action": {"const": "click"}}},
                {"required": ["action","x","y"], "properties": {"action": {"const": "double_click"}}},
                {"required": ["action","x","y"], "properties": {"action": {"const": "mouse_down"}}},
                {"required": ["action","x","y"], "properties": {"action": {"const": "mouse_up"}}},
                {"required": ["action","x","y"], "properties": {"action": {"const": "move"}}},
                {"required": ["action"], "properties": {"action": {"const": "scroll"}}},
                {"required": ["action","text"], "properties": {"action": {"const": "type_text"}}},
                {"required": ["action","name"], "properties": {"action": {"const": "key"}}}
            ]
        })
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let parsed: Input =
            serde_json::from_value(input).map_err(|e| ToolError::Input(e.to_string()))?;

        match parsed {
            Input::Screenshot {
                region,
                jpeg_quality,
            } => {
                let frame = self.screen.capture().map_err(ToolError::Other)?;
                let jpeg = encode_jpeg(&frame, region, jpeg_quality)?;
                let b64 = general_purpose::STANDARD.encode(&jpeg);
                Ok(
                    ToolOutput::ok(format!("screenshot {}x{}", frame.width, frame.height))
                        .with_metadata(json!({
                            "width": frame.width,
                            "height": frame.height,
                            "base64_jpeg": b64,
                        })),
                )
            }
            Input::Click { x, y, button } => {
                self.pointer.move_to(x, y).map_err(ToolError::Other)?;
                self.pointer
                    .click(MouseButton::from(button))
                    .map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("click at ({x}, {y})")))
            }
            Input::DoubleClick { x, y, button } => {
                self.pointer.move_to(x, y).map_err(ToolError::Other)?;
                self.pointer
                    .double_click(MouseButton::from(button))
                    .map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("double_click at ({x}, {y})")))
            }
            Input::MouseDown { x, y, button } => {
                self.pointer.move_to(x, y).map_err(ToolError::Other)?;
                self.pointer
                    .button_down(MouseButton::from(button))
                    .map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("mouse_down at ({x}, {y})")))
            }
            Input::MouseUp { x, y, button } => {
                self.pointer.move_to(x, y).map_err(ToolError::Other)?;
                self.pointer
                    .button_up(MouseButton::from(button))
                    .map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("mouse_up at ({x}, {y})")))
            }
            Input::Move { x, y } => {
                self.pointer.move_to(x, y).map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("move to ({x}, {y})")))
            }
            Input::Scroll { dx, dy } => {
                self.pointer.scroll(dx, dy).map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("scroll ({dx}, {dy})")))
            }
            Input::TypeText { text } => {
                self.keyboard.type_text(&text).map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!(
                    "typed {} chars",
                    text.chars().count()
                )))
            }
            Input::Key { name } => {
                self.keyboard.press_key(&name).map_err(ToolError::Other)?;
                Ok(ToolOutput::ok(format!("pressed {name}")))
            }
        }
    }
}

fn encode_jpeg(
    frame: &ScreenFrame,
    region: Option<Region>,
    quality: u8,
) -> Result<Vec<u8>, ToolError> {
    let buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(frame.width, frame.height, frame.pixels.clone())
            .ok_or_else(|| ToolError::Other("invalid pixel buffer".into()))?;
    let dyn_img = image::DynamicImage::ImageRgba8(buffer);

    let cropped = match region {
        Some(r) => dyn_img.crop_imm(r.x, r.y, r.width, r.height),
        None => dyn_img,
    };

    let mut out = Vec::new();
    let rgb = cropped.to_rgb8();
    let encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut out), quality);
    let (w, h) = rgb.dimensions();
    {
        use image::ImageEncoder;
        encoder
            .write_image(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| ToolError::Other(e.to_string()))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct FakeScreen {
        w: u32,
        h: u32,
    }

    impl Capture for FakeScreen {
        fn capture(&self) -> Result<ScreenFrame, String> {
            let pixels = vec![255u8; (self.w * self.h * 4) as usize];
            Ok(ScreenFrame {
                width: self.w,
                height: self.h,
                pixels,
            })
        }
    }

    #[derive(Default)]
    struct FakePointer {
        moves: AtomicUsize,
        clicks: AtomicUsize,
    }

    impl Pointer for FakePointer {
        fn move_to(&self, _x: i32, _y: i32) -> Result<(), String> {
            self.moves.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn click(&self, _b: MouseButton) -> Result<(), String> {
            self.clicks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn double_click(&self, _b: MouseButton) -> Result<(), String> {
            self.clicks.fetch_add(2, Ordering::SeqCst);
            Ok(())
        }
        fn button_down(&self, _b: MouseButton) -> Result<(), String> {
            Ok(())
        }
        fn button_up(&self, _b: MouseButton) -> Result<(), String> {
            Ok(())
        }
        fn scroll(&self, _dx: i32, _dy: i32) -> Result<(), String> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeKeyboard {
        typed: std::sync::Mutex<String>,
    }

    impl Keyboard for FakeKeyboard {
        fn type_text(&self, text: &str) -> Result<(), String> {
            self.typed.lock().expect("mutex poisoned").push_str(text);
            Ok(())
        }
        fn press_key(&self, _name: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn screenshot_returns_base64_jpeg() {
        let screen = Arc::new(FakeScreen { w: 10, h: 10 });
        let pointer = Arc::new(FakePointer::default());
        let keyboard = Arc::new(FakeKeyboard::default());
        let tool = ComputerTool::new(screen, pointer, keyboard);
        let out = tool
            .execute(json!({"action": "screenshot"}), &ToolContext::test("/tmp"))
            .await
            .unwrap();
        let meta = out.metadata.unwrap();
        assert_eq!(meta["width"], 10);
        assert!(meta["base64_jpeg"].as_str().unwrap().len() > 10);
    }

    #[tokio::test]
    async fn click_moves_then_clicks() {
        let screen = Arc::new(FakeScreen { w: 10, h: 10 });
        let pointer = Arc::new(FakePointer::default());
        let keyboard = Arc::new(FakeKeyboard::default());
        let p = pointer.clone();
        let tool = ComputerTool::new(screen, pointer, keyboard);
        tool.execute(
            json!({"action": "click", "x": 5, "y": 5}),
            &ToolContext::test("/tmp"),
        )
        .await
        .unwrap();
        assert_eq!(p.moves.load(Ordering::SeqCst), 1);
        assert_eq!(p.clicks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn type_text_routes_to_keyboard() {
        let screen = Arc::new(FakeScreen { w: 1, h: 1 });
        let pointer = Arc::new(FakePointer::default());
        let keyboard = Arc::new(FakeKeyboard::default());
        let k = keyboard.clone();
        let tool = ComputerTool::new(screen, pointer, keyboard);
        tool.execute(
            json!({"action": "type_text", "text": "hello"}),
            &ToolContext::test("/tmp"),
        )
        .await
        .unwrap();
        assert_eq!(*k.typed.lock().expect("mutex poisoned"), "hello");
    }

    #[tokio::test]
    async fn unknown_action_is_rejected() {
        let screen = Arc::new(FakeScreen { w: 1, h: 1 });
        let pointer = Arc::new(FakePointer::default());
        let keyboard = Arc::new(FakeKeyboard::default());
        let tool = ComputerTool::new(screen, pointer, keyboard);
        let err = tool
            .execute(json!({"action": "bogus"}), &ToolContext::test("/tmp"))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::Input(_)));
    }

    #[tokio::test]
    async fn screenshot_crops_to_region() {
        let screen = Arc::new(FakeScreen { w: 20, h: 20 });
        let pointer = Arc::new(FakePointer::default());
        let keyboard = Arc::new(FakeKeyboard::default());
        let tool = ComputerTool::new(screen, pointer, keyboard);
        let out = tool
            .execute(
                json!({
                    "action": "screenshot",
                    "region": {"x": 2, "y": 2, "width": 8, "height": 8}
                }),
                &ToolContext::test("/tmp"),
            )
            .await
            .unwrap();
        let meta = out.metadata.unwrap();
        assert_eq!(meta["width"], 20);
    }
}
