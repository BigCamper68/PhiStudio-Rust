use super::*;
impl Options {
    pub(crate) fn parse() -> Result<Self> {
        let mut options = Self::default();
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--workspace" => {
                    options.workspace = Some(
                        args.next()
                            .context("--workspace requires a private storage directory")?
                            .into(),
                    );
                }
                "--package" => {
                    options.package = Some(
                        args.next()
                            .context("--package requires a ZIP/PEZ path")?
                            .into(),
                    );
                }
                "--chart" => {
                    options.chart =
                        Some(args.next().context("--chart requires a JSON path")?.into())
                }
                "--smoke-test" => {
                    options.smoke = Some(
                        args.next()
                            .context("--smoke-test requires an output directory")?
                            .into(),
                    )
                }
                "--playback-smoke" => {
                    options.playback_smoke = Some(
                        args.next()
                            .context("--playback-smoke requires an output directory")?
                            .into(),
                    );
                }
                "--size" => {
                    let size = args.next().context("--size requires WIDTHxHEIGHT")?;
                    let (w, h) = size
                        .split_once('x')
                        .context("--size requires WIDTHxHEIGHT")?;
                    let (w, h) = (w.parse::<i32>()?, h.parse::<i32>()?);
                    ensure!(
                        w >= 600 && h >= 320,
                        "Minimum development viewport: 600x320"
                    );
                    options.size = Some((w, h));
                }
                "--help" | "-h" => {
                    println!(
                        "PhiStudio Rust\n  --package PATH     Import ZIP/PEZ into the project library\n  --workspace DIR   Enable persistent projects and recovery (desktop)\n  --smoke-test DIR   Render, edit, undo/redo and save; write screenshots and exit\n  --size WIDTHxHEIGHT  Development window size"
                    );
                    std::process::exit(0);
                }
                _ => anyhow::bail!("Unknown option: {arg}"),
            }
        }
        ensure!(
            options.chart.is_none() || options.package.is_none(),
            "Choose --chart or --package"
        );
        ensure!(
            options.smoke.is_none() || options.playback_smoke.is_none(),
            "Choose one smoke mode"
        );
        Ok(options)
    }
}
