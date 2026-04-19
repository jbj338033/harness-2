mod native;
mod tool;
mod traits;

pub use native::{NativeKeyboard, NativePointer, NativeScreen};
pub use tool::ComputerTool;
pub use traits::{Capture, Keyboard, MouseButton, Pointer, ScreenFrame};
