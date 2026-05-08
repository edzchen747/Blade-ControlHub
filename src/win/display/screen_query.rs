use brightness::Brightness;
use futures::stream::StreamExt;
use pollster;

/// Reads the current screen brightness from the first available display device.
pub fn get_screen_brightness() -> u8 {
    pollster::block_on(async {
        let mut dev_stream = brightness::brightness_devices();
        match dev_stream.next().await {
            Some(Ok(dev)) => dev.get().await.unwrap_or(100) as u8,
            _ => 0,
        }
    })
}
