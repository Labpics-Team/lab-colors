use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let (verdict, _) = labcolors_audit::run_full_audit(&root)?;
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, &verdict)?;
    writeln!(output)?;
    output.flush()?;
    if verdict.passed {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!(
            "Аудит не завершён успешно: {} без оценки, {} без основания, {} дефектных из {}.",
            verdict.not_assessed_count,
            verdict.orphaned_count,
            verdict.defective_count,
            verdict.total_artifacts
        );
        Ok(ExitCode::FAILURE)
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Аудит не выполнен: {error}");
            ExitCode::from(2)
        }
    }
}
