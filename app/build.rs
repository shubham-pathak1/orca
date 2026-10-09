fn write_if_changed(path: &std::path::Path, bytes: &[u8]) {
    if std::fs::read(path).ok().as_deref() != Some(bytes) {
        std::fs::write(path, bytes).expect("write generated UI asset");
    }
}

fn logo_asset(logo: &image::DynamicImage, edge: u32) -> image::DynamicImage {
    let resized = logo
        .resize(edge, edge, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = image::RgbaImage::new(edge, edge);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        ((edge - resized.width()) / 2).into(),
        ((edge - resized.height()) / 2).into(),
    );
    image::DynamicImage::ImageRgba8(canvas)
}

fn main() {
    use std::hash::{Hash, Hasher};
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    println!("cargo:rerun-if-changed=ui/assets/default-cover.png");
    let source = std::fs::read("ui/assets/default-cover.png").expect("default cover");
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hash);
    "fallback-covers-lanczos3-v1".hash(&mut hash);
    let signature = hash.finish().to_string();
    let signature_file = output.join("fallback-covers.signature");
    let unchanged =
        std::fs::read_to_string(&signature_file).ok().as_deref() == Some(signature.as_str());
    let cover =
        (!unchanged).then(|| image::load_from_memory(&source).expect("decode default cover"));
    let mut icons = String::from("export global FallbackCovers {\n");
    for edge in [32, 64, 128, 256, 512, 1024] {
        let destination = output.join(format!("default-cover-{edge}.png"));
        if let Some(cover) = &cover {
            cover
                .resize_exact(edge, edge, image::imageops::FilterType::Lanczos3)
                .save(&destination)
                .expect("encode shared fallback cover");
        } else if !destination.exists() {
            image::load_from_memory(&source)
                .expect("decode missing fallback cover")
                .resize_exact(edge, edge, image::imageops::FilterType::Lanczos3)
                .save(&destination)
                .expect("restore shared fallback cover");
        }
        let path = destination.to_str().unwrap().replace('\\', "/");
        icons.push_str(&format!(
            "    out property<image> cover-{edge}: @image-url(\"{path}\");\n"
        ));
    }
    icons.push_str("}\n");
    write_if_changed(&output.join("fallback-covers.slint"), icons.as_bytes());
    write_if_changed(&signature_file, signature.as_bytes());
    // Small sidebar logos need filtered assets rather than sampling the large
    // original at paint time. Keep the original colors and dark outline.
    println!("cargo:rerun-if-changed=ui/assets/orca-logo.png");
    let logo = image::open("ui/assets/orca-logo.png").expect("Orca logo");
    let mut logos = String::from("export global SidebarLogos {\n");
    for edge in [24, 30, 32, 36, 48, 72, 96] {
        let destination = output.join(format!("orca-sidebar-{edge}.png"));
        let resized = logo_asset(&logo, edge);
        let mut encoded = std::io::Cursor::new(Vec::new());
        resized
            .write_to(&mut encoded, image::ImageFormat::Png)
            .expect("encode sidebar logo");
        write_if_changed(&destination, encoded.get_ref());
        let path = destination.to_str().unwrap().replace('\\', "/");
        logos.push_str(&format!(
            "    out property<image> logo-{edge}: @image-url(\"{path}\");\n"
        ));
    }
    logos.push_str("}\n");
    write_if_changed(&output.join("sidebar-logos.slint"), logos.as_bytes());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=ui/assets/orca-logo.png");
        let icon = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("orca.ico");
        let frames: Vec<_> = [16, 24, 32, 48, 64, 128, 256]
            .into_iter()
            .map(|edge| {
                let resized = logo_asset(&logo, edge).to_rgba8();
                image::codecs::ico::IcoFrame::as_png(
                    resized.as_raw(),
                    edge,
                    edge,
                    image::ExtendedColorType::Rgba8,
                )
                .expect("encode Windows icon size")
            })
            .collect();
        let mut encoded = Vec::new();
        image::codecs::ico::IcoEncoder::new(&mut encoded)
            .encode_images(&frames)
            .expect("encode Windows icon");
        write_if_changed(&icon, &encoded);
        winresource::WindowsResource::new()
            .set_icon(icon.to_str().unwrap())
            .set("ProductName", "Orca")
            .set("FileDescription", "Orca Music Player")
            .set("OriginalFilename", "orca-slint.exe")
            .compile()
            .expect("Windows application resources");
    }
    let configuration = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_include_paths(vec![output]);
    slint_build::compile_with_config("ui/app.slint", configuration).expect("compile Orca UI");
}
