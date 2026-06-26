use crate::cli::InitArgs;
use crate::pipeline::config::PipelineConfig;

pub fn execute(args: InitArgs) -> crate::Result<()> {
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
