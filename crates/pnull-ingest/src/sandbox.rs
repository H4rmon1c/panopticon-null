//! Real operating-system extraction sandbox using bubblewrap.
//!
//! External PDF and OCR tools run with no network namespace, no inherited
//! secrets, no writable access outside a dedicated output directory, and
//! read-only access to the exact input and required runtime files. Address
//! space, CPU, and file-size limits are applied via `prlimit`; wall-time
//! limits are enforced by the caller; and process trees are contained in an
//! isolated PID namespace that is torn down on completion or timeout. A
//! test-only fake sandbox is provided for focused unit tests; there is no
//! unsandboxed production fallback.
//!
//! Input files are staged read-only into a private subdirectory of the
//! sandbox working directory, exposed at a dedicated read-only mount, and
//! output files are written into the writable working-directory mount, so
//! extractors never touch the host filesystem directly. This keeps the
//! sandbox usable regardless of how `/tmp` is mounted inside it.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use tempfile::TempDir;
use thiserror::Error;
use wait_timeout::ChildExt;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox backend unavailable: {0}")]
    Unavailable(String),
    #[error("allowlisted extractor is unavailable: {0}")]
    ExtractorUnavailable(String),
    #[error("sandboxed extractor failed: {0}")]
    Failed(String),
    #[error("sandboxed extractor exceeded its time limit: {0}")]
    Timeout(String),
    #[error("sandbox I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

/// Resource limits applied to every sandboxed subprocess.
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    pub max_address_bytes: u64,
    pub max_cpu_seconds: u64,
    pub max_file_size_bytes: u64,
    pub max_output_bytes: u64,
}

impl SandboxConfig {
    pub const fn defaults() -> Self {
        Self {
            max_address_bytes: 536_870_912,
            max_cpu_seconds: 12,
            max_file_size_bytes: 52_428_800,
            max_output_bytes: 5 * 1024 * 1024,
        }
    }
}

/// A sandboxed subprocess result.
#[derive(Clone, Debug)]
pub struct SandboxOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub success: bool,
}

/// A sandbox that runs external extractors in an isolated environment.
pub trait Sandbox {
    /// The dedicated writable directory bound into the sandbox. Callers place
    /// input and output files here so both the sandbox and the host can see
    /// them; this directory is the only writable host location.
    fn working_dir(&self) -> &Path;

    /// Runs `program` with `args` inside the sandbox.
    ///
    /// Every path in `inputs` is staged read-only into the sandbox and any
    /// matching argument is rewritten to the staged location. The sandbox's
    /// working directory is bound writable, so callers place output files
    /// there (visible to the host) and pass output paths that live inside it.
    fn run(
        &self,
        program: &str,
        args: &[&OsStr],
        inputs: &[&Path],
        timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError>;
}

/// The bubblewrap-backed production sandbox. Fails closed if bubblewrap or an
/// allowlisted extractor cannot be established.
pub struct BubblewrapSandbox {
    config: SandboxConfig,
    working_dir: TempDir,
}

impl BubblewrapSandbox {
    pub fn new(config: SandboxConfig) -> Result<Self, SandboxError> {
        if resolve_executable("bwrap").is_none() {
            return Err(SandboxError::Unavailable(
                "bubblewrap (bwrap) is required for live extraction".to_owned(),
            ));
        }
        let working_dir = TempDir::new()?;
        Ok(Self {
            config,
            working_dir,
        })
    }
}

impl Sandbox for BubblewrapSandbox {
    fn working_dir(&self) -> &Path {
        self.working_dir.path()
    }

    fn run(
        &self,
        program: &str,
        args: &[&OsStr],
        inputs: &[&Path],
        timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        let bwrap = resolve_executable("bwrap")
            .ok_or_else(|| SandboxError::Unavailable("bwrap".to_owned()))?;
        let executable = resolve_executable(program)
            .ok_or_else(|| SandboxError::ExtractorUnavailable(program.to_owned()))?;
        let prlimit = resolve_executable("prlimit")
            .ok_or_else(|| SandboxError::Unavailable("prlimit".to_owned()))?;

        let workdir = self.working_dir.path();
        let ro_dir = workdir.join("inputs");
        fs::create_dir_all(&ro_dir)?;

        // Stage inputs read-only into the workdir and record path rewrites to
        // the read-only `/work-ro` mount. The workdir persists across
        // subprocess runs within one sandbox, so staged copies may already
        // exist; make them writable, overwrite, then restore read-only.
        let mut input_rewrites: HashMap<PathBuf, PathBuf> = HashMap::new();
        for (index, input) in inputs.iter().enumerate() {
            let name = input.file_name().unwrap_or_else(|| OsStr::new("input"));
            let staged = ro_dir.join(format!("{index}-{}", name.to_string_lossy()));
            if staged.exists() {
                fs::set_permissions(&staged, fs::Permissions::from_mode(0o600))?;
            }
            fs::copy(input, &staged)?;
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o400))?;
            input_rewrites.insert(input.to_path_buf(), staged);
        }

        let stdout_path = workdir.join("stdout.bin");
        let stderr_path = workdir.join("stderr.bin");
        let stdout_file = fs::File::create(&stdout_path)?;
        let stderr_file = fs::File::create(&stderr_path)?;
        let mut command = Command::new(&prlimit);
        command
            .arg(format!("--as={}", self.config.max_address_bytes))
            .arg(format!("--cpu={}", self.config.max_cpu_seconds))
            .arg(format!("--fsize={}", self.config.max_file_size_bytes))
            .arg("--");
        command.arg(&bwrap);
        bind_readonly_runtime(&mut command);
        command
            // The workdir is the only writable host location, mounted at a
            // dedicated `/work` path so it is never masked by `/tmp`.
            .arg("--bind")
            .arg(workdir)
            .arg("/work")
            // Staged inputs are exposed read-only at `/work-ro`.
            .arg("--ro-bind")
            .arg(&ro_dir)
            .arg("/work-ro")
            .arg("--dev")
            .arg("/dev")
            .arg("--proc")
            .arg("/proc")
            .arg("--tmpfs")
            .arg("/tmp")
            .arg("--unshare-net")
            .arg("--unshare-pid")
            .arg("--unshare-ipc")
            .arg("--unshare-uts")
            .arg("--new-session")
            .arg("--clearenv")
            .arg("--setenv")
            .arg("LANG")
            .arg("C.UTF-8")
            .arg("--setenv")
            .arg("LC_ALL")
            .arg("C.UTF-8")
            .arg("--setenv")
            .arg("OMP_THREAD_LIMIT")
            .arg("1")
            .arg("--")
            .arg(&executable)
            .args(rewrite_args(args, workdir, &input_rewrites))
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        let mut child = command.spawn().map_err(SandboxError::Io)?;
        let status = match child.wait_timeout(timeout) {
            Ok(Some(status)) => status,
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SandboxError::Timeout(program.to_owned()));
            }
            Err(error) => return Err(SandboxError::Io(error)),
        };
        let success = status.success();
        let stdout = read_limited(&stdout_path, self.config.max_output_bytes)?;
        let stderr = read_limited(&stderr_path, self.config.max_output_bytes)?;
        if !success {
            return Err(SandboxError::Failed(program.to_owned()));
        }
        Ok(SandboxOutput {
            stdout,
            stderr,
            success,
        })
    }
}

/// A test-only fake sandbox that runs extractors directly. Never used for
/// production ingestion.
pub struct FakeSandbox {
    config: SandboxConfig,
    working_dir: TempDir,
}

impl FakeSandbox {
    pub fn new(config: SandboxConfig) -> Result<Self, SandboxError> {
        Ok(Self {
            config,
            working_dir: TempDir::new()?,
        })
    }
}

impl Sandbox for FakeSandbox {
    fn working_dir(&self) -> &Path {
        self.working_dir.path()
    }

    fn run(
        &self,
        program: &str,
        args: &[&OsStr],
        _inputs: &[&Path],
        timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        let executable = resolve_executable(program)
            .ok_or_else(|| SandboxError::ExtractorUnavailable(program.to_owned()))?;
        let workdir = self.working_dir.path();
        let stdout_path = workdir.join("stdout.bin");
        let stderr_path = workdir.join("stderr.bin");
        let stdout = fs::File::create(&stdout_path)?;
        let stderr = fs::File::create(&stderr_path)?;
        let mut command = Command::new(&executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .env_clear()
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .env("OMP_THREAD_LIMIT", "1");
        let mut child = command.spawn().map_err(SandboxError::Io)?;
        let Some(status) = child.wait_timeout(timeout).map_err(SandboxError::Io)? else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SandboxError::Timeout(program.to_owned()));
        };
        let success = status.success();
        let stdout = fs::read(&stdout_path)?;
        let stderr = fs::read(&stderr_path)?;
        if !success {
            return Err(SandboxError::Failed(program.to_owned()));
        }
        let _ = self.config.max_output_bytes;
        Ok(SandboxOutput {
            stdout,
            stderr,
            success,
        })
    }
}

fn bind_readonly_runtime(command: &mut Command) {
    for directory in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/nix"] {
        if Path::new(directory).exists() {
            command.arg("--ro-bind").arg(directory).arg(directory);
        }
    }
}

/// Rewrites sandboxed argument paths: staged inputs become `/work-ro/<name>`
/// and paths under the host workdir become `/work/<relative>`; anything else
/// is returned unchanged.
fn rewrite_args(
    args: &[&OsStr],
    workdir: &Path,
    input_rewrites: &HashMap<PathBuf, PathBuf>,
) -> Vec<std::ffi::OsString> {
    args.iter()
        .map(|arg| {
            let path = Path::new(arg);
            if let Some(staged) = input_rewrites.get(path) {
                let name = staged.file_name().unwrap_or_else(|| OsStr::new("input"));
                // The staged input lives in the read-only `/work-ro` mount.
                return std::ffi::OsString::from(format!("/work-ro/{}", name.to_string_lossy()));
            }
            if let Ok(relative) = path.strip_prefix(workdir) {
                return std::ffi::OsString::from(format!("/work/{}", relative.to_string_lossy()));
            }
            arg.to_os_string()
        })
        .collect()
}

fn read_limited(path: &Path, max_bytes: u64) -> Result<Vec<u8>, SandboxError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > max_bytes {
        return Ok(Vec::new());
    }
    Ok(fs::read(path)?)
}

fn resolve_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake() -> FakeSandbox {
        FakeSandbox::new(SandboxConfig::defaults()).expect("fake sandbox")
    }

    #[test]
    fn fake_sandbox_runs_an_extractor() {
        let sandbox = fake();
        let output = sandbox
            .run("echo", &[OsStr::new("hello")], &[], Duration::from_secs(5))
            .expect("run");
        assert!(output.success);
        assert_eq!(output.stdout, b"hello\n");
    }

    #[test]
    fn missing_extractor_fails_closed() {
        let sandbox = fake();
        let result = sandbox.run(
            "definitely-not-a-real-tool-xyz",
            &[],
            &[],
            Duration::from_secs(5),
        );
        assert!(matches!(result, Err(SandboxError::ExtractorUnavailable(_))));
    }

    fn real_sandbox_available() -> Option<BubblewrapSandbox> {
        BubblewrapSandbox::new(SandboxConfig::defaults()).ok()
    }

    fn shell(
        sandbox: &dyn Sandbox,
        script: &str,
        timeout: Duration,
    ) -> Result<SandboxOutput, SandboxError> {
        sandbox.run(
            "/bin/sh",
            &[OsStr::new("-c"), OsStr::new(script)],
            &[],
            timeout,
        )
    }

    #[test]
    fn sandboxed_tool_cannot_read_an_unrelated_host_file() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        // /home is not bound into the sandbox, so the file does not exist there.
        let result = shell(
            &sandbox,
            "cat /home/owo/definitely-secret.txt",
            Duration::from_secs(10),
        );
        assert!(result.is_err(), "unrelated file must be unreachable");
    }

    #[test]
    fn sandboxed_tool_cannot_write_outside_its_output_directory() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        // /etc is bound read-only, so a write there must fail.
        let result = shell(
            &sandbox,
            "echo x > /etc/panopticon-write-test",
            Duration::from_secs(10),
        );
        assert!(result.is_err(), "write outside workdir must fail");
    }

    #[test]
    fn sandboxed_tool_has_no_host_network_routes() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        // In the isolated network namespace the routing table is empty.
        let output =
            shell(&sandbox, "cat /proc/net/route", Duration::from_secs(10)).expect("route read");
        assert!(output.success);
        let text = String::from_utf8_lossy(&output.stdout);
        // Only the header line, or nothing: no host routes leak in.
        assert!(
            text.lines()
                .filter(|line| !line.trim().is_empty() && !line.starts_with("Iface"))
                .count()
                == 0,
            "host routes must not leak into the sandbox network namespace"
        );
    }

    #[test]
    fn sandboxed_tool_that_never_terminates_is_killed_on_timeout() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        // An unbounded process loop must be contained and killed by the timeout.
        let result = shell(
            &sandbox,
            "while true; do true; done",
            Duration::from_millis(500),
        );
        assert!(matches!(result, Err(SandboxError::Timeout(_))));
    }
}
