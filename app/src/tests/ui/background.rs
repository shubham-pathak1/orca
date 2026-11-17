use super::*;
use crate::artwork::BackgroundBlend;

slint::slint! {
    export component BlendComparison inherits Window {
        width:96px;height:48px;
        in property<image> original;
        in property<image> prepared;
        in property<color> base;
        in property<float> strength;
        background:root.base;
        Rectangle {
            x:0px;y:0px;width:48px;height:48px;
            Image {x:0px;y:0px;width:parent.width;height:parent.height;source:root.original;image-fit:fill;opacity:root.strength;}
            Rectangle {x:0px;y:0px;width:parent.width;height:parent.height;background:root.base.with-alpha(0.8);}
        }
        Image {x:48px;width:48px;height:48px;source:root.prepared;image-fit:fill;}
    }
}

#[test]
fn prepared_library_background_matches_translucent_software_layers() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(OffscreenPlatform(window.clone()))).unwrap();
    let ui = BlendComparison::new().unwrap();
    window.set_size(slint::PhysicalSize::new(96, 48));
    let source: Vec<u8> = (0..32 * 32)
        .flat_map(|index| [(index * 17) as u8, (index * 31) as u8, index as u8])
        .collect();
    ui.set_original(slint::Image::from_rgb8(
        slint::SharedPixelBuffer::clone_from_slice(&source, 32, 32),
    ));
    ui.show().unwrap();
    for light in [false, true] {
        let blend = BackgroundBlend::library(light);
        let prepared: Vec<u8> = source
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|pixel| blend.apply([pixel[0], pixel[1], pixel[2]]))
            .collect();
        ui.set_base(slint::Color::from_rgb_u8(
            blend.base[0],
            blend.base[1],
            blend.base[2],
        ));
        ui.set_strength(if light { 0.08 } else { 0.24 });
        ui.set_prepared(slint::Image::from_rgb8(
            slint::SharedPixelBuffer::clone_from_slice(&prepared, 32, 32),
        ));
        let mut pixels = vec![slint::Rgb8Pixel::default(); 96 * 48];
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 96);
        });
        for row in pixels.as_chunks::<96>().0.iter() {
            for (before, after) in row[..48].iter().zip(&row[48..]) {
                assert_eq!(before, after, "background changed in light={light}");
            }
        }
    }
    ui.hide().unwrap();
}
