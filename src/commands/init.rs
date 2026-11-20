use crate::cli::InitArgs;
use qsmxt_engine::PipelineConfig;

pub fn execute(args: InitArgs) -> qsmxt_io::Result<()> {
    let config = PipelineConfig::default();
    let toml_str = config.to_toml().map_err(|e| qsmxt_io::error::QsmxtError::Config(e.to_string()))?;

    match args.output {
        Some(path) => {
            fs_err::write(&path, &toml_str)?;
            println!("Configuration written to {}", path.display());
        }
        None => {
            print!("{}", toml_str);
        }
    }

    Ok(())
}
