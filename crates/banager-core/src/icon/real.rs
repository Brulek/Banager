//! `RealIconRenderer`: the icon Finder shows for an app, drawn by AppKit.
//!
//! One question to the system and one drawing. `NSWorkspace iconForFile:`
//! answers with the icon Finder shows for the item at a path -- for an
//! app, the one its bundle declares -- and that image is drawn into a
//! bitmap `ICON_PIXELS` square, which AppKit encodes as PNG
//! (`NSBitmapImageRep representationUsingType:properties:`). Banager opens
//! nothing in the bundle itself: macOS finds the icon, in the bundle or in
//! its own icon cache. The bitmap and the PNG live in memory; the PNG goes
//! back to `AppIcons`, and nothing is written anywhere.
//!
//! Off the main thread, on purpose: `AppIcons` runs on a thread of tokio's
//! blocking pool (`ipc::artifact_icon` in src-tauri), and a drawing must
//! not wait for the window's own thread. None of the classes used here is
//! one the macOS SDK confines to the main thread -- its headers put
//! `NS_SWIFT_UI_ACTOR` on `NSResponder`, so on every view and window, and
//! on none of `NSWorkspace`, `NSImage`, `NSBitmapImageRep` and
//! `NSGraphicsContext` -- which is why objc2-app-kit's bindings let this
//! code call them here with no main-thread marker. A thread that draws
//! gets a graphics context of its own, and flushes it itself (Apple's
//! "Thread Safety Summary" for AppKit): the context below is this
//! drawing's own, made from its own bitmap, and flushed before it is let
//! go. The image is made, drawn and dropped on this one thread, and
//! `AppIcons` makes one drawing at a time.

use super::IconRenderer;
use std::path::Path;

/// macOS's own icons. Stateless: the shared workspace is AppKit's, and
/// every drawing makes its own bitmap. Built once, by `AppIcons::real`.
#[derive(Debug, Default)]
pub struct RealIconRenderer;

impl RealIconRenderer {
    pub fn new() -> RealIconRenderer {
        RealIconRenderer
    }
}

#[cfg(target_os = "macos")]
impl IconRenderer for RealIconRenderer {
    fn render_png(&self, bundle: &Path) -> Option<Vec<u8>> {
        use super::ICON_PIXELS;
        use objc2::rc::autoreleasepool;
        use objc2::AnyThread;
        use objc2_app_kit::{
            NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSDeviceRGBColorSpace,
            NSGraphicsContext, NSImageInterpolation, NSWorkspace,
        };
        use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

        // `NSString` carries UTF-8, and macOS's file systems do not create
        // a name that is not; such a path simply gets no icon.
        let utf8 = bundle.to_str()?;
        let pixels = ICON_PIXELS as isize;
        let square = NSSize::new(f64::from(ICON_PIXELS), f64::from(ICON_PIXELS));
        // A pool of its own, as `RealTrasher` has: this runs on a thread of
        // tokio's blocking pool, which has none, and the image, its
        // representations and the PNG's bytes come back autoreleased. The
        // bytes are copied out before it drains.
        autoreleasepool(|_| {
            let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(utf8));
            // SAFETY: `planes` is null, which asks AppKit to allocate the
            // buffer itself (8 bits per sample, 4 samples -- red, green,
            // blue, alpha -- interleaved, rows as long as AppKit likes: the
            // two zeros), and `NSDeviceRGBColorSpace` is AppKit's own
            // constant, read as the extern static it is.
            let bitmap = unsafe {
                NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    std::ptr::null_mut(),
                    pixels,
                    pixels,
                    8,
                    4,
                    true,
                    false,
                    NSDeviceRGBColorSpace,
                    0,
                    0,
                )
            }?;
            // One point per pixel, so the square drawn below covers the
            // bitmap exactly, and the image picks the representation drawn
            // for that many pixels.
            bitmap.setSize(square);
            let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
            // Nothing below returns early until the state is restored.
            NSGraphicsContext::saveGraphicsState_class();
            NSGraphicsContext::setCurrentContext(Some(&context));
            context.setImageInterpolation(NSImageInterpolation::High);
            // The whole image (a zero source rect) into the whole square,
            // `Copy` so every pixel is the icon's -- transparent where the
            // icon is -- whatever the new buffer held.
            icon.drawInRect_fromRect_operation_fraction(
                NSRect::new(NSPoint::new(0.0, 0.0), square),
                NSRect::ZERO,
                NSCompositingOperation::Copy,
                1.0,
            );
            context.flushGraphics();
            NSGraphicsContext::restoreGraphicsState_class();
            // SAFETY: an empty dictionary, so there is no property whose
            // value could be of the wrong type.
            let png = unsafe {
                bitmap.representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
            }?;
            Some(png.to_vec())
        })
    }
}

/// No Finder, no icon: Banager v0.1 ships for macOS only (the crate doc in
/// lib.rs), and a row does without one.
#[cfg(not(target_os = "macos"))]
impl IconRenderer for RealIconRenderer {
    fn render_png(&self, _bundle: &Path) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::icon::ICON_PIXELS;

    /// A system app every Mac has: its icon is read, and nothing is
    /// written anywhere.
    const CALCULATOR: &str = "/System/Applications/Calculator.app";

    /// Draws a real icon with AppKit, so it is left out of the default run
    /// like this workspace's other tests that reach the real system. It
    /// only reads: run it with
    /// `cargo test -p banager-core --lib icon::real -- --ignored`.
    #[test]
    #[ignore = "draws Calculator's icon with AppKit; run with cargo test -p banager-core --lib icon::real -- --ignored"]
    fn test_real_renderer_draws_calculators_icon_as_a_png_across_the_whole_square() {
        // On a thread of its own, as on tokio's blocking pool: never the
        // main thread.
        let png = std::thread::spawn(|| RealIconRenderer::new().render_png(Path::new(CALCULATOR)))
            .join()
            .expect("the drawing thread did not panic")
            .expect("macOS gave Calculator an icon");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "not a PNG");

        let mut reader = png::Decoder::new(std::io::Cursor::new(&png[..]))
            .read_info()
            .expect("a PNG png can read");
        let mut pixels = vec![0; reader.output_buffer_size().expect("a size that fits")];
        let frame = reader.next_frame(&mut pixels).expect("its pixels");
        assert_eq!((frame.width, frame.height), (ICON_PIXELS, ICON_PIXELS));
        assert_eq!(frame.color_type, png::ColorType::Rgba);
        assert_eq!(frame.bit_depth, png::BitDepth::Eight);

        // Drawn across the whole square, not into a corner of it and not
        // blank: Calculator's icon fills its square but for a margin, so
        // most of each quarter is not transparent.
        let half = ICON_PIXELS as usize / 2;
        for (qx, qy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let seen = (0..half)
                .flat_map(|y| (0..half).map(move |x| (qx * half + x, qy * half + y)))
                .filter(|&(x, y)| pixels[y * frame.line_size + x * 4 + 3] > 0)
                .count();
            assert!(
                seen > half * half / 2,
                "only {seen} of the {} pixels in quarter ({qx}, {qy}) are drawn",
                half * half
            );
        }
    }
}
