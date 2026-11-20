use crate::cli::InitArgs;
use qsmxt_engine::PipelineConfig;

pub fn execute(args: InitArgs) -> qsmxt_io::Result<()> {
    let config = PipelineConfig::default();
    let toml_str = config.to_toml().unwrap_or_default();

    match args.output {
        Some(path) => {
            std::fs::write(&path, &toml_str)?;
            println!("Configuration written to {}", path.display());
        }
        None => {
            print!("{}", toml_str);
        }
    }

    Ok(())
}
