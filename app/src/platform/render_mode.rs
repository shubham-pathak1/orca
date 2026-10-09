//! Keep the CPU baseline explicit even when GPU support is compiled in.
#[derive(Debug, PartialEq)]
pub enum Mode {
    Cpu,
    Gpu,
}
pub fn requested(args: &[String]) -> Result<Mode, String> {
    let gpu = args.iter().any(|arg| arg == "--gpu");
    let cpu = args.iter().any(|arg| arg == "--cpu");
    if gpu && cpu {
        return Err("Choose either --gpu or --cpu".into());
    }
    Ok(if gpu { Mode::Gpu } else { Mode::Cpu })
}
pub fn renderer(mode: &Mode, gpu_available: bool) -> Result<&'static str, String> {
    match mode {
        Mode::Cpu => Ok("software"),
        Mode::Gpu if gpu_available => Ok("femtovg"),
        Mode::Gpu => Err("This binary has no GPU renderer. Build with --features gpu.".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabling_gpu_support_keeps_cpu_as_default() {
        let mode = requested(&["orca-slint".into()]).unwrap();
        assert_eq!(mode, Mode::Cpu);
        assert_eq!(renderer(&mode, true).unwrap(), "software");
        assert_eq!(
            renderer(&requested(&["--gpu".into()]).unwrap(), true).unwrap(),
            "femtovg"
        );
    }
    #[test]
    fn unavailable_or_conflicting_gpu_requests_fail_instead_of_mislabeling_cpu() {
        assert!(renderer(&Mode::Gpu, false).is_err());
        assert!(requested(&["--gpu".into(), "--cpu".into()]).is_err());
    }
}
