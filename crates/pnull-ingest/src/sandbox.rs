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
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use tempfile::TempDir;
use thiserror::Error;
use wait_timeout::ChildExt;

/// The component in the sandbox pipeline that reported a failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureLayer {
    /// `prlimit` could not launch the sandboxed pipeline.
    Prlimit,
    /// Bubblewrap could not set up the sandbox (namespace/mount setup failed).
    Bubblewrap,
    /// The extractor itself (for example Poppler's `pdftotext`) exited non-zero.
    Extractor,
}

/// Structured evidence retained when a sandboxed extractor fails. The exit
/// status and stderr are never discarded, so the operator can tell why a
/// sandboxed run failed and whether the fault was in `prlimit`, Bubblewrap, or
/// the extractor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractorFailure {
    /// The allowlisted extractor that was invoked (e.g. `pdftotext`).
    pub extractor: String,
    /// Which component of the pipeline failed.
    pub layer: FailureLayer,
    /// The exit code, when the process exited normally with a status.
    pub exit_code: Option<i32>,
    /// The terminating signal number, when the process was killed by a signal.
    pub signal: Option<i32>,
    /// Bounded, sanitized stderr captured from the failed run.
    pub stderr: String,
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox backend unavailable: {0}")]
    Unavailable(String),
    #[error("allowlisted extractor is unavailable: {0}")]
    ExtractorUnavailable(String),
    #[error("sandboxed extractor failed: {0}")]
    Failed(ExtractorFailure),
    #[error("sandboxed extractor exceeded its time limit: {0}")]
    Timeout(String),
    #[error("sandbox I/O failure: {0}")]
    Io(#[from] std::io::Error),
}

impl std::fmt::Display for ExtractorFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let layer = match self.layer {
            FailureLayer::Prlimit => "prlimit",
            FailureLayer::Bubblewrap => "bubblewrap",
            FailureLayer::Extractor => "extractor",
        };
        let status = match (self.exit_code, self.signal) {
            (Some(code), _) => format!("exit {code}"),
            (None, Some(sig)) => format!("signal {sig}"),
            (None, None) => "no exit status".to_owned(),
        };
        write!(f, "{} (layer {layer}, {status})", self.extractor)?;
        if !self.stderr.is_empty() {
            write!(f, " — stderr: {}", self.stderr)?;
        }
        Ok(())
    }
}

/// Resource limits applied to every sandboxed subprocess.
#[derive(Clone, Debug)]
pub struct SandboxConfig {
    pub max_address_bytes: u64,
    pub max_cpu_seconds: u64,
    pub max_file_size_bytes: u64,
    pub max_output_bytes: u64,
    /// Strict page-only mode binds only a single known output file writable.
    pub page_mode: bool,
}

impl SandboxConfig {
    pub const fn defaults() -> Self {
        Self {
            max_address_bytes: 536_870_912,
            max_cpu_seconds: 12,
            max_file_size_bytes: 52_428_800,
            max_output_bytes: 5 * 1024 * 1024,
            page_mode: false,
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
        // The writable /work mount must never contain staged inputs. A
        // compromised extractor could otherwise replace one with a symlink
        // that the host follows during the next run.
        let (ro_dir, input_rewrites) = stage_inputs(inputs)?;

        // Capture files are also outside /work: an extractor must not be able
        // to plant a symlink that a later host-side File::create will follow.
        let capture_dir = TempDir::new()?;
        let stdout_path = capture_dir.path().join("stdout.bin");
        let stderr_path = capture_dir.path().join("stderr.bin");
        let stdout_file = fs::File::create(&stdout_path)?;
        let stderr_file = fs::File::create(&stderr_path)?;
        let mut command = Command::new(&prlimit);
        command
            .arg(format!("--as={}", self.config.max_address_bytes))
            .arg(format!("--cpu={}", self.config.max_cpu_seconds))
            .arg(format!("--fsize={}", self.config.max_file_size_bytes));
        if self.config.page_mode {
            command.arg("--nofile=64");
        }
        command.arg("--");
        command.arg(&bwrap);
        bind_readonly_runtime(&mut command);
        bind_workdir(&mut command, self.config.page_mode, program, args, workdir)?;
        command
            // Staged inputs are exposed read-only at `/work-ro`.
            .arg("--ro-bind")
            .arg(ro_dir.path())
            .arg("/work-ro");
        bind_devices(&mut command, self.config.page_mode);
        command
            .arg("--proc")
            .arg("/proc")
            .arg("--size")
            .arg(self.config.max_file_size_bytes.to_string())
            .arg("--tmpfs")
            .arg("/tmp")
            .arg("--unshare-net")
            .arg("--unshare-pid")
            .arg("--unshare-ipc")
            .arg("--unshare-uts")
            .arg("--new-session")
            .arg("--die-with-parent")
            .arg("--clearenv")
            .arg("--setenv")
            .arg("LANG")
            .arg("C.UTF-8")
            .arg("--setenv")
            .arg("LC_ALL")
            .arg("C.UTF-8")
            .arg("--setenv")
            .arg("OMP_THREAD_LIMIT")
            .arg("1");
        if self.config.page_mode {
            // Prevent a compromised extractor from creating a fresh user/mount
            // namespace and remounting paths that were made read-only above.
            command.arg("--unshare-user").arg("--disable-userns");
            protect_page_mounts(&mut command);
        }
        // Only a directory already exposed by the read-only runtime mounts
        // may be passed through; no other host environment is inherited.
        if program == "tesseract"
            && let Some(raw) = std::env::var_os("PNUL_TESSDATA_DIR")
        {
            command
                .arg("--setenv")
                .arg("TESSDATA_PREFIX")
                .arg(checked_tessdata_dir(Path::new(&raw))?);
        }
        command
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
            let stderr_text = sanitize_stderr(&stderr, self.config.max_output_bytes);
            return Err(SandboxError::Failed(ExtractorFailure {
                extractor: program.to_owned(),
                layer: classify_layer(&stderr_text),
                exit_code: status.code(),
                signal: status.signal(),
                stderr: stderr_text,
            }));
        }
        Ok(SandboxOutput {
            stdout,
            stderr,
            success,
        })
    }
}

fn bind_devices(command: &mut Command, page_mode: bool) {
    if page_mode {
        // Do not expose ptmx/devpts: even a read-only dev mount would allow
        // allocation of host pseudo-terminal resources.
        command
            .arg("--size")
            .arg("1048576")
            .arg("--tmpfs")
            .arg("/dev");
        command.arg("--dev-bind").arg("/dev/null").arg("/dev/null");
        for device in ["/dev/zero", "/dev/random", "/dev/urandom"] {
            command.arg("--ro-bind").arg(device).arg(device);
        }
    } else {
        command.arg("--dev").arg("/dev");
    }
}

fn protect_page_mounts(command: &mut Command) {
    // No arbitrary writable directories; /dev/null remains usable.
    for path in ["/tmp", "/dev", "/"] {
        command.arg("--remount-ro").arg(path);
    }
}

fn bind_workdir(
    command: &mut Command,
    page_mode: bool,
    program: &str,
    args: &[&OsStr],
    workdir: &Path,
) -> Result<(), SandboxError> {
    let output = if page_mode {
        page_output_file(program, args, workdir)?
    } else {
        None
    };
    command
        .arg(if page_mode { "--ro-bind" } else { "--bind" })
        .arg(workdir)
        .arg("/work");
    if let Some(output) = output.as_ref() {
        let relative = output
            .strip_prefix(workdir)
            .map_err(|_| SandboxError::Unavailable("output escapes workdir".to_owned()))?;
        command
            .arg("--bind")
            .arg(output)
            .arg(Path::new("/work").join(relative));
    }
    Ok(())
}

fn page_output_file(
    program: &str,
    args: &[&OsStr],
    workdir: &Path,
) -> Result<Option<PathBuf>, SandboxError> {
    use std::os::unix::fs::OpenOptionsExt;
    let output = match program {
        "pdfinfo" => None,
        "pdftotext" if args.last().is_some_and(|arg| *arg == OsStr::new("-")) => None,
        "pdftoppm" if args.iter().any(|arg| *arg == OsStr::new("-singlefile")) => {
            let prefix = args
                .last()
                .ok_or_else(|| SandboxError::Unavailable("missing render output".to_owned()))?;
            Some(Path::new(prefix).with_extension("png"))
        }
        "tesseract"
            if args
                .iter()
                .any(|arg| *arg == OsStr::new("tessedit_create_tsv=1")) =>
        {
            let base = args
                .get(1)
                .ok_or_else(|| SandboxError::Unavailable("missing OCR output".to_owned()))?;
            let mut name = base.to_os_string();
            name.push(".tsv");
            Some(PathBuf::from(name))
        }
        #[cfg(test)]
        "sh" | "/bin/sh" => None,
        _ => {
            return Err(SandboxError::Unavailable(
                "extractor is not allowed in page mode".to_owned(),
            ));
        }
    };
    if let Some(path) = output.as_ref() {
        let relative = path
            .strip_prefix(workdir)
            .map_err(|_| SandboxError::Unavailable("output outside workdir".to_owned()))?;
        if relative.as_os_str().is_empty()
            || !relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(SandboxError::Unavailable(
                "output outside workdir".to_owned(),
            ));
        }
        let parent = path
            .parent()
            .ok_or_else(|| SandboxError::Unavailable("missing output directory".to_owned()))?;
        for component in parent.ancestors().take_while(|p| *p != workdir) {
            let meta = fs::symlink_metadata(component)?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(SandboxError::Unavailable("unsafe output parent".to_owned()));
            }
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
    }
    Ok(output)
}

fn stage_inputs(inputs: &[&Path]) -> Result<(TempDir, HashMap<PathBuf, PathBuf>), SandboxError> {
    let ro_dir = TempDir::new()?;
    let mut rewrites = HashMap::new();
    for (index, input) in inputs.iter().enumerate() {
        require_regular_file_without_symlinks(input)?;
        let name = input.file_name().unwrap_or_else(|| OsStr::new("input"));
        let staged = ro_dir
            .path()
            .join(format!("{index}-{}", name.to_string_lossy()));
        fs::copy(input, &staged)?;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o400))?;
        rewrites.insert((*input).to_path_buf(), staged);
    }
    Ok((ro_dir, rewrites))
}

pub(crate) fn require_regular_file_without_symlinks(path: &Path) -> Result<(), SandboxError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(SandboxError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "sandbox input must be a regular file",
        )));
    }
    for ancestor in path.ancestors().filter(|part| !part.as_os_str().is_empty()) {
        if fs::symlink_metadata(ancestor)?.file_type().is_symlink() {
            return Err(SandboxError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "sandbox input path must not contain a symlink",
            )));
        }
    }
    Ok(())
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
            let stderr_text = sanitize_stderr(&stderr, self.config.max_output_bytes);
            return Err(SandboxError::Failed(ExtractorFailure {
                extractor: program.to_owned(),
                layer: classify_layer(&stderr_text),
                exit_code: status.code(),
                signal: status.signal(),
                stderr: stderr_text,
            }));
        }
        let _ = self.config.max_output_bytes;
        Ok(SandboxOutput {
            stdout,
            stderr,
            success,
        })
    }
}

fn checked_tessdata_dir(path: &Path) -> Result<PathBuf, SandboxError> {
    let resolved = fs::canonicalize(path).map_err(|_| {
        SandboxError::Unavailable("configured OCR language directory is unavailable".to_owned())
    })?;
    if !(resolved.starts_with("/usr") || resolved.starts_with("/nix/store"))
        || !resolved.is_dir()
        || !fs::symlink_metadata(resolved.join("eng.traineddata"))
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return Err(SandboxError::Unavailable(
            "configured OCR language data must be in a read-only runtime directory".to_owned(),
        ));
    }
    Ok(resolved)
}

fn bind_readonly_runtime(command: &mut Command) {
    command.arg("--tmpfs").arg("/etc");
    for path in readonly_runtime_paths() {
        if Path::new(path).exists() {
            command.arg("--ro-bind").arg(path).arg(path);
        }
    }
    command.arg("--remount-ro").arg("/etc");
}

fn readonly_runtime_paths() -> &'static [&'static str] {
    // Read-only is not a confidentiality boundary: mounting all of /etc
    // exposes any service-readable credentials to a compromised extractor.
    // Font configuration and the dynamic linker cache are the only required
    // /etc inputs. Likewise expose immutable Nix packages, not its state.
    &[
        "/usr",
        "/bin",
        "/lib",
        "/lib64",
        "/etc/fonts",
        "/etc/ld.so.cache",
        "/nix/store",
    ]
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

/// Sanitizes captured stderr for operator display: bounds it to `max_bytes`
/// after UTF-8 lossy decoding, trims surrounding whitespace, and strips control characters so
/// terminal escape sequences and other non-printable bytes never reach a
/// terminal or log.
fn sanitize_stderr(bytes: &[u8], max_bytes: u64) -> String {
    let lossy = String::from_utf8_lossy(bytes);
    let mut cleaned: String = String::new();
    for ch in lossy.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            // len() is constant-time and counts UTF-8 bytes. Counting chars
            // on each iteration made attacker-controlled stderr quadratic
            // work in the host process after sandbox CPU limits had ended.
            if cleaned.len() as u64 + ch.len_utf8() as u64 > max_bytes {
                break;
            }
            cleaned.push(ch);
        }
    }
    cleaned.trim().to_owned()
}

/// Classifies which component of the sandbox pipeline failed. The pipeline is
/// `prlimit -- bwrap ... -- <extractor>`; a non-zero exit is normally the
/// extractor's status (Bubblewrap propagates the child's exit code), but a
/// Bubblewrap setup failure produces its own distinctive namespace/mount error
/// in stderr, which we attribute to Bubblewrap.
fn classify_layer(stderr: &str) -> FailureLayer {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("prlimit:")
        || lower.contains("prlimit ")
        || lower.contains("failed to set resource limits")
    {
        FailureLayer::Prlimit
    } else if lower.contains("bwrap:")
        || lower.contains("creating new namespace")
        || lower.contains("unshare")
        || lower.contains("mount")
        || lower.contains("operation not permitted")
    {
        FailureLayer::Bubblewrap
    } else {
        FailureLayer::Extractor
    }
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
    use std::os::unix::fs::symlink;

    fn fake() -> FakeSandbox {
        FakeSandbox::new(SandboxConfig::defaults()).expect("fake sandbox")
    }

    #[test]
    fn stderr_sanitization_is_byte_bounded_after_lossy_decode() {
        assert_eq!(sanitize_stderr(b"abc", 0), "");
        assert_eq!(sanitize_stderr(b"abcdef", 3), "abc");
        assert_eq!(sanitize_stderr("éé".as_bytes(), 3), "é");
        assert_eq!(sanitize_stderr(&[0xff; 10], 5), "\u{fffd}");
        assert_eq!(sanitize_stderr(b"\x1b\x00 warning\r\n", 100), "warning");
        let noisy = vec![b'x'; 5 * 1024 * 1024];
        assert_eq!(sanitize_stderr(&noisy, 1024).len(), 1024);
    }

    #[test]
    fn runtime_mounts_exclude_system_secrets_and_nix_state() {
        let paths = readonly_runtime_paths();
        assert!(!paths.contains(&"/etc"));
        assert!(!paths.contains(&"/nix"));
        assert!(
            paths
                .iter()
                .filter(|path| path.starts_with("/etc/"))
                .all(|path| { matches!(*path, "/etc/fonts" | "/etc/ld.so.cache") })
        );
    }

    #[test]
    fn ocr_language_data_cannot_expand_the_sandbox_read_boundary() {
        let outside = TempDir::new().expect("temporary directory");
        fs::write(outside.path().join("eng.traineddata"), b"fixture").expect("fixture");
        assert!(checked_tessdata_dir(outside.path()).is_err());
        assert!(checked_tessdata_dir(Path::new("/etc")).is_err());
        assert!(checked_tessdata_dir(Path::new("/definitely-missing-ocr-data")).is_err());
    }

    #[test]
    fn staged_inputs_are_private_and_reject_symlink_traps() {
        let writable = TempDir::new().expect("writable directory");
        let input = writable.path().join("input.pdf");
        fs::write(&input, b"original").expect("input");
        let (staging, rewrites) = stage_inputs(&[&input]).expect("stage input");
        assert!(!staging.path().starts_with(writable.path()));
        assert_eq!(
            fs::read(rewrites.get(&input).expect("staged path")).expect("staged bytes"),
            b"original"
        );
        let sentinel = TempDir::new().expect("sentinel directory");
        let target = sentinel.path().join("private.txt");
        fs::write(&target, b"private").expect("sentinel");
        fs::remove_file(&input).expect("remove input");
        symlink(&target, &input).expect("trap");
        assert!(stage_inputs(&[&input]).is_err());
        assert_eq!(fs::read(&target).expect("sentinel bytes"), b"private");

        let parent = writable.path().join("aliased");
        symlink(sentinel.path(), &parent).expect("parent trap");
        assert!(stage_inputs(&[&parent.join("private.txt")]).is_err());
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

    #[test]
    fn failed_run_retains_extractor_exit_status_and_stderr() {
        let sandbox = fake();
        // A real tool that exits non-zero and writes to stderr.
        let result = sandbox.run(
            "/bin/sh",
            &[OsStr::new("-c"), OsStr::new("echo boom >&2; exit 7")],
            &[],
            Duration::from_secs(5),
        );
        match result {
            Err(SandboxError::Failed(failure)) => {
                assert_eq!(failure.extractor, "/bin/sh");
                assert_eq!(failure.exit_code, Some(7));
                assert_eq!(failure.signal, None);
                assert!(failure.stderr.contains("boom"), "stderr preserved");
                assert_eq!(failure.layer, FailureLayer::Extractor);
            }
            other => panic!("expected Failed with structured evidence, got {other:?}"),
        }
    }

    #[test]
    fn bubblewrap_setup_failure_is_attributed_to_bubblewrap() {
        // A pipeline that fails during sandbox setup produces bwrap-specific
        // stderr; the layer must be Bubblewrap, not Extractor.
        let sandbox = fake();
        let result = sandbox.run(
            "/bin/sh",
            &[OsStr::new("-c"), OsStr::new("echo 'bwrap: Creating new namespace failed: Operation not permitted' >&2; exit 1")],
            &[],
            Duration::from_secs(5),
        );
        match result {
            Err(SandboxError::Failed(failure)) => {
                assert_eq!(failure.layer, FailureLayer::Bubblewrap);
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    fn real_sandbox_available() -> Option<BubblewrapSandbox> {
        BubblewrapSandbox::new(SandboxConfig::defaults()).ok()
    }

    fn page_sandbox_available() -> bool {
        if resolve_executable("unshare").is_none() {
            return false;
        }
        let Some(bwrap) = resolve_executable("bwrap") else {
            return false;
        };
        let Ok(help) = Command::new(bwrap).arg("--help").output() else {
            return false;
        };
        if !String::from_utf8_lossy(&help.stdout).contains("--disable-userns") {
            return false;
        }
        real_sandbox_available().is_some_and(|sandbox| host_shell_available(&sandbox))
    }

    // The sandbox tests bind a host `/bin/sh` into the isolated namespace. In
    // restricted build environments (for example the Nix derivation sandbox)
    // the host shell cannot be bound into the nested namespace, so the probe
    // cannot be constructed. Detect that by actually running a trivial shell
    // command through the sandbox and skip the test when it cannot run.
    fn host_shell_available(sandbox: &dyn Sandbox) -> bool {
        shell(sandbox, "true", Duration::from_secs(10)).is_ok()
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
    fn sandbox_capture_files_ignore_workdir_symlink_traps() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        if !host_shell_available(&sandbox) {
            eprintln!("SKIP: shell unavailable inside bubblewrap");
            return;
        }
        let sentinel = TempDir::new().expect("sentinel directory");
        for name in ["stdout.bin", "stderr.bin"] {
            let target = sentinel.path().join(name);
            fs::write(&target, b"private").expect("sentinel");
            symlink(&target, sandbox.working_dir().join(name)).expect("trap");
        }
        let output = shell(
            &sandbox,
            "printf public; printf warning >&2",
            Duration::from_secs(5),
        )
        .expect("sandbox run");
        assert_eq!(output.stdout, b"public");
        assert_eq!(output.stderr, b"warning");
        for name in ["stdout.bin", "stderr.bin"] {
            assert_eq!(
                fs::read(sentinel.path().join(name)).expect("sentinel bytes"),
                b"private"
            );
        }
    }

    #[test]
    fn sandboxed_tool_cannot_read_an_unrelated_host_file() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        if !host_shell_available(&sandbox) {
            eprintln!("SKIP: no host /bin/sh to bind");
            return;
        }
        // /home is not bound into the sandbox, so the file does not exist there.
        let result = shell(
            &sandbox,
            "cat /home/owo/definitely-secret.txt",
            Duration::from_secs(10),
        );
        assert!(result.is_err(), "unrelated file must be unreachable");
    }

    #[test]
    fn sandboxed_tool_cannot_read_host_etc_configuration() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        if !host_shell_available(&sandbox) {
            eprintln!("SKIP: no host /bin/sh to bind");
            return;
        }
        assert!(Path::new("/etc/passwd").exists(), "host fixture exists");
        shell(
            &sandbox,
            "test ! -e /etc/passwd && test ! -e /etc/panopticon && test ! -e /nix/var",
            Duration::from_secs(10),
        )
        .expect("host configuration must not be mounted");
    }

    #[test]
    fn page_mode_blocks_new_files_in_all_visible_directories() {
        if !page_sandbox_available() {
            return;
        }
        let mut config = SandboxConfig::defaults();
        config.page_mode = true;
        let sandbox = BubblewrapSandbox::new(config).expect("page sandbox");
        let script = "for p in /work/new /work-ro/new /tmp/new /dev/new /dev/pts/new /proc/new /etc/new /new; do if touch $p 2>/dev/null; then exit 1; fi; done; printf ok";
        let result = shell(&sandbox, script, Duration::from_secs(10)).expect("restricted sandbox");
        assert_eq!(result.stdout, b"ok");
        let no_pty = shell(
            &sandbox,
            r#"test ! -e /dev/ptmx && test ! -e /dev/pts/ptmx && test $(ulimit -n) -le 64 && command -v unshare >/dev/null && printf ok >/dev/null || exit 1; output=$(unshare --user --map-root-user --mount --fork true 2>&1); status=$?; test $status -ne 0 && printf '%s' "$output" | grep -Eq 'Operation not permitted|No space left on device|Too many users'"#,
            Duration::from_secs(10),
        )
        .expect("no pseudo-terminal allocator and bounded file descriptors");
        assert!(no_pty.success);
        assert!(!sandbox.working_dir().join("new").exists());
    }

    #[test]
    #[ignore = "subprocess helper, invoked by parent-death test"]
    fn page_mode_parent_death_worker() {
        if std::env::var_os("PNUL_PAGE_DEATH_WORKER").is_none() {
            return;
        }
        let mut config = SandboxConfig::defaults();
        config.page_mode = true;
        let sandbox = BubblewrapSandbox::new(config).expect("page sandbox");
        let _ = shell(&sandbox, "exec sleep 30", Duration::from_secs(35));
    }

    #[test]
    fn page_mode_tears_down_when_parent_dies() {
        if !page_sandbox_available() {
            return;
        }
        let mut parent = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "sandbox::tests::page_mode_parent_death_worker",
                "--ignored",
            ])
            .env("PNUL_PAGE_DEATH_WORKER", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("worker");
        let parent_pid = parent.id();
        let mut sandbox_pid = None;
        for _ in 0..100 {
            let tasks = PathBuf::from(format!("/proc/{parent_pid}/task"));
            if let Ok(entries) = fs::read_dir(tasks) {
                for entry in entries.flatten() {
                    if let Ok(children) = fs::read_to_string(entry.path().join("children")) {
                        for candidate in children.split_whitespace() {
                            let name = fs::read_to_string(format!("/proc/{candidate}/comm"))
                                .unwrap_or_default();
                            if name.trim() == "bwrap" {
                                sandbox_pid = candidate.parse::<u32>().ok();
                            }
                        }
                    }
                }
            }
            if sandbox_pid.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let sandbox_pid = sandbox_pid.expect("worker started bubblewrap");
        parent.kill().expect("kill worker");
        parent.wait().expect("reap worker");
        for _ in 0..100 {
            match fs::read_to_string(format!("/proc/{sandbox_pid}/stat")) {
                Err(_) => return,
                Ok(status) if status.contains(") Z ") => return,
                _ => std::thread::sleep(Duration::from_millis(25)),
            }
        }
        panic!("bubblewrap survived its parent");
    }

    #[test]
    fn page_output_rejects_path_escape_and_existing_file() {
        let dir = TempDir::new().expect("workdir");
        let outside = dir.path().join("..").join("escape");
        let outside_arg = outside.as_os_str();
        assert!(
            page_output_file(
                "pdftoppm",
                &[OsStr::new("-singlefile"), outside_arg],
                dir.path()
            )
            .is_err()
        );
        let prefix = dir.path().join("page");
        fs::write(prefix.with_extension("png"), b"existing").expect("fixture");
        assert!(
            page_output_file(
                "pdftoppm",
                &[OsStr::new("-singlefile"), prefix.as_os_str()],
                dir.path()
            )
            .is_err()
        );
        assert_eq!(fs::read(prefix.with_extension("png")).unwrap(), b"existing");
    }

    #[test]
    fn sandboxed_tool_cannot_write_outside_its_output_directory() {
        let Some(sandbox) = real_sandbox_available() else {
            eprintln!("SKIP: bubblewrap unavailable");
            return;
        };
        if !host_shell_available(&sandbox) {
            eprintln!("SKIP: no host /bin/sh to bind");
            return;
        }
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
        // In some restricted build environments (for example the Nix derivation
        // sandbox) the nested namespace cannot read /proc/net/route at all, so
        // the probe cannot run. Skip rather than failing the security check
        // that the routing table holds no host routes.
        let Ok(output) = shell(&sandbox, "cat /proc/net/route", Duration::from_secs(10)) else {
            eprintln!("SKIP: cannot read /proc/net/route in this environment");
            return;
        };
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
        if !host_shell_available(&sandbox) {
            eprintln!("SKIP: no host /bin/sh to bind");
            return;
        }
        // An unbounded process loop must be contained and killed by the timeout.
        let result = shell(
            &sandbox,
            "while true; do true; done",
            Duration::from_millis(500),
        );
        assert!(matches!(result, Err(SandboxError::Timeout(_))));
    }
}
