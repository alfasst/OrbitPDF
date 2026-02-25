use cosmic::widget::image::Handle;
use cosmic::iced::widget::image;
fn foo() {
    let bytes = vec![0u8; 100 * 100 * 4];
    let handle1 = Handle::from_rgba(100, 100, bytes.clone());
    let handle2 = Handle::from_pixels(100, 100, bytes);
}
