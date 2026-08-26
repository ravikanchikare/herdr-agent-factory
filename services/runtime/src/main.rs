use std::collections::HashSet;
use std::env;
use std::ffi::OsStr;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use agent_factory_runtime::{
    AgentControlService, EnvironmentServicePaths, RUNTIME_NAME, RUNTIME_VERSION, Runtime,
    application_data_directory,
};
use ipc_contract::{Frame, PROTOCOL_VERSION, Ready, read_frame, write_frame};
use project_store::ProjectStore;

fn main() {
    if let Err(error) = run() {
        eprintln!("agent-factory-runtime: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let data_directory = application_data_directory()?;
    let store = ProjectStore::open(data_directory.join("agent-factory.sqlite3"))?;
    let search_paths = agent_search_paths();
    let mut runtime = Runtime::with_environment_services(
        store,
        search_paths,
        EnvironmentServicePaths {
            user_environments: data_directory.join("environments"),
            plugins: data_directory.join("plugins"),
        },
        runtime_secret_store()?,
    )?;

    // An Orchestrator drives its own Factory Run through this socket. Failing to
    // bind is not fatal: sessions still start, they just cannot advance a Run on
    // their own, and the reason belongs on stderr rather than in a crash.
    let control = match AgentControlService::bind(&data_directory) {
        Ok(service) => {
            runtime.set_control_endpoint(service.endpoint().to_path_buf());
            Some(service)
        }
        Err(error) => {
            eprintln!("agent-factory-runtime: agent control is unavailable: {error}");
            None
        }
    };

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    let (input_tx, input_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("runtime-stdin".into())
        .spawn(move || {
            let stdin = io::stdin();
            let mut reader = BufReader::new(stdin.lock());
            loop {
                let frame = read_frame(&mut reader);
                let finished = matches!(frame, Ok(None) | Err(_));
                if input_tx.send(frame).is_err() || finished {
                    break;
                }
            }
        })?;

    write_frame(
        &mut writer,
        &Frame::Ready(Ready {
            version: PROTOCOL_VERSION,
            runtime_name: RUNTIME_NAME.into(),
            runtime_version: RUNTIME_VERSION.into(),
        }),
    )?;

    loop {
        for event in runtime.poll_events() {
            write_frame(&mut writer, &event)?;
        }
        if let Some(control) = control.as_ref() {
            for event in runtime.drain_agent_control(control) {
                write_frame(&mut writer, &event)?;
            }
        }
        let frame = match input_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(frame) => frame?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => None,
        };
        match frame {
            Some(Frame::Request(request)) => {
                for frame in runtime.handle_request(request) {
                    write_frame(&mut writer, &frame)?;
                }
            }
            Some(Frame::Shutdown(_)) | None => break,
            Some(Frame::Hello(_)) => {
                write_frame(
                    &mut writer,
                    &Frame::Ready(Ready {
                        version: PROTOCOL_VERSION,
                        runtime_name: RUNTIME_NAME.into(),
                        runtime_version: RUNTIME_VERSION.into(),
                    }),
                )?;
            }
            Some(_) => {
                return Err("runtime received a frame that only it may emit".into());
            }
        }
    }

    Ok(())
}

fn agent_search_paths() -> Vec<PathBuf> {
    let inherited = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();

    #[cfg(target_os = "macos")]
    let login = login_shell::search_paths();
    #[cfg(not(target_os = "macos"))]
    let login = Vec::new();

    merge_search_paths(login, inherited)
}

fn merge_search_paths(
    primary: impl IntoIterator<Item = PathBuf>,
    fallback: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    primary
        .into_iter()
        .chain(fallback)
        .filter(|path| path.is_absolute() && seen.insert(path.clone()))
        .collect()
}

/// Everything a Finder-launched macOS binary needs to learn the user's real
/// PATH from their login shell.
#[cfg(target_os = "macos")]
mod login_shell {
    use std::env;
    use std::ffi::{OsStr, OsString};
    use std::io::{self, Read, Seek};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(2);
    const MAX_LOGIN_PATH_BYTES: u64 = 64 * 1024;

    pub(super) fn search_paths() -> Vec<PathBuf> {
        let shell = env::var_os("SHELL")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/bin/zsh"));
        let environment = ["HOME", "USER", "LOGNAME", "SHELL", "LANG"]
            .into_iter()
            .filter_map(|name| env::var_os(name).map(|value| (OsString::from(name), value)))
            .chain(env::vars_os().filter(|(name, _)| name.as_encoded_bytes().starts_with(b"LC_")))
            .collect::<Vec<_>>();
        query_login_shell_path(&shell, &environment, LOGIN_SHELL_TIMEOUT).unwrap_or_default()
    }

    fn query_login_shell_path(
        shell: &Path,
        environment: &[(OsString, OsString)],
        timeout: Duration,
    ) -> io::Result<Vec<PathBuf>> {
        if !shell.is_absolute() || !shell.is_file() {
            return Ok(Vec::new());
        }

        // A Finder-launched application receives only the system PATH. Ask the
        // user's configured login shell for its PATH once, with a fixed command,
        // a minimal environment, bounded output, and a startup deadline. Agent
        // processes are still launched directly by absolute path, never through
        // the shell.
        let mut output = tempfile::tempfile()?;
        let child_output = output.try_clone()?;
        let mut command = Command::new(shell);
        command
            .args(["-l", "-c", "/usr/bin/printenv PATH"])
            .env_clear()
            .envs(environment.iter().cloned())
            .stdin(Stdio::null())
            .stdout(Stdio::from(child_output))
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait()? {
                Some(status) if status.success() => break,
                Some(_) => return Ok(Vec::new()),
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(Vec::new());
                }
            }
        }

        output.rewind()?;
        let mut bytes = Vec::new();
        output
            .take(MAX_LOGIN_PATH_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_LOGIN_PATH_BYTES {
            return Ok(Vec::new());
        }
        let value = std::str::from_utf8(&bytes).ok().map(str::trim);
        Ok(value
            .filter(|value| !value.is_empty())
            .map(|value| env::split_paths(OsStr::new(value)).collect())
            .unwrap_or_default())
    }

    #[cfg(test)]
    mod tests {
        use std::fs;

        use super::*;

        #[test]
        fn login_shell_path_is_split_without_interpreting_output_as_commands() {
            use std::os::unix::fs::PermissionsExt;

            let directory = tempfile::tempdir().unwrap();
            let shell = directory.path().join("login-shell");
            fs::write(&shell, "#!/bin/sh\nprintf '/user/bin:/opt/tools/bin\\n'\n").unwrap();
            let mut permissions = fs::metadata(&shell).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&shell, permissions).unwrap();

            assert_eq!(
                query_login_shell_path(&shell, &[], Duration::from_secs(1)).unwrap(),
                [PathBuf::from("/user/bin"), PathBuf::from("/opt/tools/bin")]
            );
        }
    }
}

fn runtime_secret_store()
-> Result<Arc<dyn platform_secrets::SecretStore>, Box<dyn std::error::Error>> {
    // The stdio integration test executes the real debug binary. Keep it away
    // from the developer's Keychain without creating a release-build override.
    #[cfg(debug_assertions)]
    if env::var_os("AGENT_FACTORY_TEST_IN_MEMORY_SECRETS").as_deref() == Some(OsStr::new("1")) {
        return Ok(Arc::new(platform_secrets::InMemorySecretStore::default()));
    }

    production_secret_store()
}

#[cfg(target_os = "macos")]
fn production_secret_store()
-> Result<Arc<dyn platform_secrets::SecretStore>, Box<dyn std::error::Error>> {
    Ok(Arc::new(platform_secrets::MacOsKeychain::new(
        "app.agentfactory.desktop",
    )?))
}

#[cfg(not(target_os = "macos"))]
fn production_secret_store()
-> Result<Arc<dyn platform_secrets::SecretStore>, Box<dyn std::error::Error>> {
    Ok(Arc::new(platform_secrets::InMemorySecretStore::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_paths_prefer_login_entries_and_reject_relative_entries() {
        assert_eq!(
            merge_search_paths(
                [PathBuf::from("/user/bin"), PathBuf::from("relative")],
                [PathBuf::from("/system/bin"), PathBuf::from("/user/bin")],
            ),
            [PathBuf::from("/user/bin"), PathBuf::from("/system/bin")]
        );
    }
}
