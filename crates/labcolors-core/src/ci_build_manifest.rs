use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
        if path.extension().and_then(|s| s.to_str()) == Some("yml") {
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

fn parse_workflow(file: &str, content: &str) -> Result<WorkflowEntry, String> {
    let doc: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error in {file}: {e}"))?;

    let name = doc
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let triggers = extract_triggers(&doc);
    let jobs = extract_jobs(&doc);

    Ok(WorkflowEntry {
        file: file.to_string(),
        name,
        triggers,
        jobs,
    })
}

fn extract_triggers(doc: &serde_yaml::Value) -> Vec<String> {
    let on = match doc.get("on") {
        Some(v) => v,
        None => return Vec::new(),
    };

    match on {
        serde_yaml::Value::String(s) => vec![s.clone()],
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        serde_yaml::Value::Mapping(map) => map
            .keys()
            .filter_map(|k| k.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_jobs(doc: &serde_yaml::Value) -> Vec<JobEntry> {
    let jobs_map = match doc.get("jobs").and_then(|v| v.as_mapping()) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut jobs = Vec::new();
    for (key, value) in jobs_map {
        let id = key.as_str().unwrap_or("").to_string();
        let mapping = match value.as_mapping() {
            Some(m) => m,
            None => continue,
        };

        let name = mapping
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();

        let runs_on = mapping
            .get("runs-on")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let timeout_minutes = mapping
            .get("timeout-minutes")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        let needs = match mapping.get("needs") {
            Some(serde_yaml::Value::String(s)) => vec![s.clone()],
            Some(serde_yaml::Value::Sequence(seq)) => seq
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => Vec::new(),
        };

        // A job is a required check if it has no `if:` condition that could skip it
        // and is not gated by `continue-on-error`. Simplified heuristic: jobs with
        // `uses:` (reusable workflow calls) are always required.
        let is_required_check =
            mapping.get("uses").is_some() || mapping.get("continue-on-error").is_none();

        let steps = extract_steps(mapping);
        let matrix = extract_matrix(mapping);

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
    jobs
}

fn extract_steps(job: &serde_yaml::Mapping) -> Vec<StepEntry> {
    let steps_seq = match job.get("steps").and_then(|v| v.as_sequence()) {
        Some(s) => s,
        None => return Vec::new(),
    };

    steps_seq
        .iter()
        .filter_map(|step| {
            let m = step.as_mapping()?;
            Some(StepEntry {
                name: m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                uses: m
                    .get("uses")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                run: m.get("run").and_then(|v| v.as_str()).map(|s| s.to_string()),
            })
        })
        .collect()
}

fn extract_matrix(job: &serde_yaml::Mapping) -> Option<MatrixStrategy> {
    let strategy = job.get("strategy")?.as_mapping()?;
    let matrix = strategy.get("matrix")?.as_mapping()?;

    let mut axes = HashMap::new();
    for (key, value) in matrix {
        let axis_name = key.as_str()?.to_string();
        if axis_name == "include" || axis_name == "exclude" {
            continue;
        }
        let values: Vec<String> = value
            .as_sequence()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !values.is_empty() {
            axes.insert(axis_name, values);
        }
    }

    let fail_fast = strategy
        .get("fail-fast")
        .and_then(|v| v.as_bool())
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

    #[test]
    fn extract_triggers_from_mapping() {
        let yaml = "on:\n  push:\n    branches: [main]\n  pull_request:\n";
        let doc: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let triggers = extract_triggers(&doc);
        assert!(triggers.contains(&"push".to_string()));
        assert!(triggers.contains(&"pull_request".to_string()));
    }

    #[test]
    fn extract_triggers_from_string() {
        let yaml = "on: push\n";
        let doc: serde_yaml::Value = serde_yaml::from_str(yaml).unwrap();
        let triggers = extract_triggers(&doc);
        assert_eq!(triggers, vec!["push"]);
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
}
