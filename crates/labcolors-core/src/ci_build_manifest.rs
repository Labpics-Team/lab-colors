use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiBuildManifest {
    pub workflows: Vec<WorkflowEntry>,
    pub build: BuildConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEntry {
    pub file: String,
    pub name: String,
    pub triggers: Vec<String>,
    pub jobs: Vec<JobEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEntry {
    pub id: String,
    pub name: String,
    pub runs_on: Option<String>,
    pub timeout_minutes: Option<u32>,
    pub needs: Vec<String>,
    pub is_required_check: bool,
    pub steps: Vec<StepEntry>,
    pub matrix: Option<MatrixStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepEntry {
    pub name: Option<String>,
    pub uses: Option<String>,
    pub run: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixStrategy {
    pub axes: HashMap<String, Vec<String>>,
    pub fail_fast: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub workspace_members: Vec<String>,
    pub toolchain_channel: String,
    pub worker_pairs: Vec<WorkerPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPair {
    pub caller: String,
    pub worker: String,
}

/// Extract CI/build manifest from the repository root.
/// Tier A only: standalone structure extraction without EXT-01 integration.
pub fn extract_ci_build(repo_root: &Path) -> Result<CiBuildManifest, String> {
    let workflows_dir = repo_root.join(".github").join("workflows");
    if !workflows_dir.is_dir() {
        return Err(format!(
            "workflows directory not found: {}",
            workflows_dir.display()
        ));
    }

    let mut workflow_files: Vec<String> = Vec::new();
    let entries = std::fs::read_dir(&workflows_dir)
        .map_err(|e| format!("failed to read workflows dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("dir entry error: {e}"))?;
        let path = entry.path();
        if matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("yml" | "yaml")
        ) {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                workflow_files.push(name.to_string());
            }
        }
    }
    workflow_files.sort();

    let mut workflows = Vec::new();
    for file in &workflow_files {
        let content = std::fs::read_to_string(workflows_dir.join(file))
            .map_err(|e| format!("failed to read {file}: {e}"))?;
        let wf = parse_workflow(file, &content)?;
        workflows.push(wf);
    }

    let build = extract_build_config(repo_root, &workflow_files)?;

    Ok(CiBuildManifest { workflows, build })
}

fn field<'a>(value: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    value.as_hash()?.get(&Yaml::String(key.to_owned()))
}

fn parse_workflow(file: &str, content: &str) -> Result<WorkflowEntry, String> {
    let documents = YamlLoader::load_from_str(content)
        .map_err(|error| format!("YAML parse error in {file}: {error}"))?;
    let [doc] = documents.as_slice() else {
        return Err(format!(
            "workflow {file} must contain exactly one YAML document"
        ));
    };
    if !doc.is_hash() {
        return Err(format!("workflow {file} must be a YAML mapping"));
    }

    let name = field(doc, "name")
        .and_then(Yaml::as_str)
        .unwrap_or("")
        .to_string();
    let triggers = extract_triggers(doc);
    let jobs = extract_jobs(doc).map_err(|error| format!("workflow {file}: {error}"))?;

    Ok(WorkflowEntry {
        file: file.to_string(),
        name,
        triggers,
        jobs,
    })
}

fn extract_triggers(doc: &Yaml) -> Vec<String> {
    match field(doc, "on") {
        Some(Yaml::String(value)) => vec![value.clone()],
        Some(Yaml::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        Some(Yaml::Hash(values)) => values
            .keys()
            .filter_map(|key| key.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_jobs(doc: &Yaml) -> Result<Vec<JobEntry>, String> {
    let Some(jobs_map) = field(doc, "jobs").and_then(Yaml::as_hash) else {
        return Ok(Vec::new());
    };
    let mut jobs = Vec::new();
    for (key, value) in jobs_map {
        let id = key.as_str().unwrap_or("").to_string();
        if !value.is_hash() {
            return Err(format!("job {id} must be a YAML mapping"));
        }
        let name = field(value, "name")
            .and_then(Yaml::as_str)
            .unwrap_or(&id)
            .to_string();
        let runs_on = field(value, "runs-on")
            .and_then(Yaml::as_str)
            .map(str::to_owned);
        let timeout_minutes = match field(value, "timeout-minutes") {
            None => None,
            Some(Yaml::String(expression))
                if expression.trim().starts_with("${{") && expression.trim().ends_with("}}") =>
            {
                // The host evaluates expressions; this projection records literal minutes only.
                None
            }
            Some(timeout) => Some(
                timeout
                    .as_i64()
                    .and_then(|number| u32::try_from(number).ok())
                    .ok_or_else(|| format!("job {id} timeout-minutes must be a u32 integer"))?,
            ),
        };
        let needs = match field(value, "needs") {
            Some(Yaml::String(name)) => vec![name.clone()],
            Some(Yaml::Array(names)) => names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_owned))
                .collect(),
            _ => Vec::new(),
        };

        // Structural hint only; repository rules own required-check policy.
        let is_required_check =
            field(value, "uses").is_some() || field(value, "continue-on-error").is_none();
        let steps = extract_steps(value);
        let matrix = extract_matrix(value);
        jobs.push(JobEntry {
            id,
            name,
            runs_on,
            timeout_minutes,
            needs,
            is_required_check,
            steps,
            matrix,
        });
    }
    Ok(jobs)
}

fn extract_steps(job: &Yaml) -> Vec<StepEntry> {
    let Some(steps) = field(job, "steps").and_then(Yaml::as_vec) else {
        return Vec::new();
    };
    steps
        .iter()
        .filter_map(|step| {
            step.as_hash()?;
            Some(StepEntry {
                name: field(step, "name")
                    .and_then(Yaml::as_str)
                    .map(str::to_owned),
                uses: field(step, "uses")
                    .and_then(Yaml::as_str)
                    .map(str::to_owned),
                run: field(step, "run").and_then(Yaml::as_str).map(str::to_owned),
            })
        })
        .collect()
}

fn extract_matrix(job: &Yaml) -> Option<MatrixStrategy> {
    let strategy = field(job, "strategy")?;
    let matrix = field(strategy, "matrix")?.as_hash()?;
    let mut axes = HashMap::new();
    for (key, value) in matrix {
        let axis_name = key.as_str()?.to_string();
        if axis_name == "include" || axis_name == "exclude" {
            continue;
        }
        let values: Vec<String> = value
            .as_vec()
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect();
        if !values.is_empty() {
            axes.insert(axis_name, values);
        }
    }
    let fail_fast = field(strategy, "fail-fast")
        .and_then(Yaml::as_bool)
        .unwrap_or(true);
    if axes.is_empty() {
        None
    } else {
        Some(MatrixStrategy { axes, fail_fast })
    }
}

fn extract_build_config(
    repo_root: &Path,
    workflow_files: &[String],
) -> Result<BuildConfig, String> {
    let workspace_members = extract_workspace_members(repo_root)?;
    let toolchain_channel = extract_toolchain(repo_root);
    let worker_pairs = extract_worker_pairs(workflow_files);

    Ok(BuildConfig {
        workspace_members,
        toolchain_channel,
        worker_pairs,
    })
}

fn extract_workspace_members(repo_root: &Path) -> Result<Vec<String>, String> {
    let cargo_toml_path = repo_root.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml_path)
        .map_err(|e| format!("failed to read Cargo.toml: {e}"))?;

    let doc: toml::Value =
        toml::from_str(&content).map_err(|e| format!("TOML parse error: {e}"))?;

    let members: Vec<String> = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Expand glob patterns like "crates/*" into actual directories
    let mut expanded = Vec::new();
    for member in members {
        if member.contains('*') {
            let pattern = repo_root.join(&member);
            if let Some(parent) = pattern.parent() {
                if let Ok(entries) = std::fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        if entry.path().is_dir() {
                            if let Some(name) = entry.file_name().to_str() {
                                let relative = format!(
                                    "{}/{}",
                                    parent.strip_prefix(repo_root).unwrap_or(parent).display(),
                                    name
                                );
                                expanded.push(relative.replace('\\', "/"));
                            }
                        }
                    }
                }
            }
        } else {
            expanded.push(member);
        }
    }
    expanded.sort();
    Ok(expanded)
}

fn extract_toolchain(repo_root: &Path) -> String {
    // Try rust-toolchain file first
    let toolchain_file = repo_root.join("rust-toolchain");
    let toolchain_toml = repo_root.join("rust-toolchain.toml");

    for path in [&toolchain_toml, &toolchain_file] {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(doc) = toml::from_str::<toml::Value>(&content) {
                if let Some(channel) = doc
                    .get("toolchain")
                    .and_then(|t| t.get("channel"))
                    .and_then(|c| c.as_str())
                {
                    return channel.to_string();
                }
            }
            // Plain text rust-toolchain file
            let trimmed = content.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('[') {
                return trimmed.to_string();
            }
        }
    }

    // Fallback: scan ci-worker.yml for RUST_TOOLCHAIN env var
    let ci_worker = repo_root
        .join(".github")
        .join("workflows")
        .join("ci-worker.yml");
    if let Ok(content) = std::fs::read_to_string(&ci_worker) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("RUST_TOOLCHAIN:") {
                let value = trimmed
                    .strip_prefix("RUST_TOOLCHAIN:")
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !value.is_empty() {
                    return value;
                }
            }
        }
    }

    String::new()
}

fn extract_worker_pairs(workflow_files: &[String]) -> Vec<WorkerPair> {
    let mut pairs = Vec::new();
    let worker_suffix = "-worker.yml";

    for file in workflow_files {
        if file.ends_with(worker_suffix) {
            let base = file.strip_suffix(".yml").unwrap();
            let caller = format!("{}.yml", base.strip_suffix("-worker").unwrap());
            if workflow_files.contains(&caller) {
                pairs.push(WorkerPair {
                    caller,
                    worker: file.clone(),
                });
            }
        }
    }
    pairs.sort_by(|a, b| a.caller.cmp(&b.caller));
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn extract_triggers_from_mapping() -> TestResult {
        let yaml = "on:\n  push:\n    branches: [main]\n  pull_request:\n";
        let workflow = parse_workflow("ci.yml", yaml)?;
        let triggers = workflow.triggers;
        assert!(triggers.contains(&"push".to_string()));
        assert!(triggers.contains(&"pull_request".to_string()));
        Ok(())
    }

    #[test]
    fn extract_triggers_from_string() -> TestResult {
        let yaml = "on: push\n";
        let workflow = parse_workflow("ci.yml", yaml)?;
        let triggers = workflow.triggers;
        assert_eq!(triggers, vec!["push"]);
        Ok(())
    }

    #[test]
    fn worker_pair_detection() {
        let files = vec![
            "ci.yml".to_string(),
            "ci-worker.yml".to_string(),
            "mutation.yml".to_string(),
            "mutation-worker.yml".to_string(),
            "standalone.yml".to_string(),
        ];
        let pairs = extract_worker_pairs(&files);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].caller, "ci.yml");
        assert_eq!(pairs[0].worker, "ci-worker.yml");
        assert_eq!(pairs[1].caller, "mutation.yml");
        assert_eq!(pairs[1].worker, "mutation-worker.yml");
    }

    #[test]
    fn yaml12_on_key_and_job_fields_survive_projection() -> TestResult {
        let workflow = parse_workflow(
            "ci.yml",
            r#"name: CI
on: [push, pull_request]
jobs:
  verify:
    name: Verify
    runs-on: ubuntu-latest
    timeout-minutes: 12
    needs: [build, lint]
    strategy:
      fail-fast: false
      matrix:
        target: [native, wasm]
    steps:
      - name: Inspect
        run: |
          printf 'on: off\n'
      - uses: owner/action@commit
"#,
        )?;
        assert_eq!(workflow.name, "CI");
        assert_eq!(workflow.triggers, ["push", "pull_request"]);
        let [job] = workflow.jobs.as_slice() else {
            panic!("the workflow contains one job");
        };
        assert_eq!(job.id, "verify");
        assert_eq!(job.name, "Verify");
        assert_eq!(job.runs_on.as_deref(), Some("ubuntu-latest"));
        assert_eq!(job.timeout_minutes, Some(12));
        assert_eq!(job.needs, ["build", "lint"]);
        assert_eq!(job.steps[0].run.as_deref(), Some("printf 'on: off\\n'\n"));
        assert_eq!(job.steps[1].uses.as_deref(), Some("owner/action@commit"));
        let matrix = job.matrix.as_ref().ok_or("matrix was lost")?;
        assert!(!matrix.fail_fast);
        assert_eq!(matrix.axes["target"], ["native", "wasm"]);
        Ok(())
    }

    #[test]
    fn yaml_aliases_preserve_the_declared_sequence() -> TestResult {
        let workflow = parse_workflow(
            "ci.yml",
            "on: push\nx-needs: &needs [build, lint]\njobs:\n  verify:\n    needs: *needs\n",
        )?;
        assert_eq!(workflow.jobs[0].needs, ["build", "lint"]);
        Ok(())
    }

    #[test]
    fn ambiguous_or_incomplete_documents_are_rejected() {
        for source in [
            "",
            "null",
            "[]",
            "on: push\non: pull_request\n",
            "on: push\n---\non: pull_request\n",
            "jobs: [",
            "jobs:\n  verify: *missing\n",
            "jobs:\n  verify: []\n",
        ] {
            assert!(parse_workflow("ci.yml", source).is_err(), "{source:?}");
        }
    }

    #[test]
    fn timeout_projection_cannot_wrap_or_drop_invalid_values() -> TestResult {
        for invalid in ["-1", "4294967296", "18446744073709551615", "1.5", "null"] {
            let source = format!("jobs:\n  verify:\n    timeout-minutes: {invalid}\n");
            assert!(parse_workflow("ci.yml", &source).is_err(), "{invalid}");
        }
        let workflow = parse_workflow(
            "ci.yml",
            "jobs:\n  verify:\n    timeout-minutes: 4294967295\n",
        )?;
        assert_eq!(workflow.jobs[0].timeout_minutes, Some(u32::MAX));
        Ok(())
    }

    #[test]
    fn timeout_expressions_remain_owned_by_the_host() -> TestResult {
        let workflow = parse_workflow(
            "ci.yml",
            "jobs:\n  verify:\n    timeout-minutes: ${{ inputs.timeout }}\n",
        )?;
        assert_eq!(workflow.jobs[0].timeout_minutes, None);
        Ok(())
    }

    #[test]
    fn repository_workflows_use_the_public_extraction_path() -> TestResult {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("workspace root is unavailable")?;
        let manifest = extract_ci_build(root)?;
        let entries =
            std::fs::read_dir(root.join(".github/workflows"))?.collect::<Result<Vec<_>, _>>()?;
        let expected = entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry
                        .path()
                        .extension()
                        .and_then(|extension| extension.to_str()),
                    Some("yml" | "yaml")
                )
            })
            .count();
        assert!(expected > 0);
        assert_eq!(manifest.workflows.len(), expected);
        assert!(manifest.workflows.iter().any(|workflow| {
            workflow.file == "ci.yml"
                && workflow.triggers.contains(&"pull_request".to_owned())
                && !workflow.jobs.is_empty()
        }));
        Ok(())
    }
}
