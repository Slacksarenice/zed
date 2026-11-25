use std::ffi::OsStr;
#[cfg(not(target_os = "macos"))]
use std::path::Path;

#[cfg(target_os = "macos")]
mod darwin;

#[cfg(target_os = "macos")]
pub use darwin::{Child, Command, Stdio};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000_u32;

pub use gpui_util::new_std_command;

pub fn new_command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

#[cfg(not(target_os = "macos"))]
pub type Child = smol::process::Child;

#[cfg(not(target_os = "macos"))]
pub use std::process::Stdio;

#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub struct Command(smol::process::Command);

#[cfg(not(target_os = "macos"))]
impl Command {
    #[inline]
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        #[cfg(target_os = "windows")]
        {
            use smol::process::windows::CommandExt;
            let mut cmd = smol::process::Command::new(program);
            cmd.creation_flags(CREATE_NO_WINDOW);
            Self(cmd)
        }
        #[cfg(not(target_os = "windows"))]
        Self(smol::process::Command::new(program))
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.0.arg(arg);
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.0.args(args);
        self
    }

    pub fn get_args(&self) -> impl Iterator<Item = &OsStr> {
        self.0.get_args()
    }

    pub fn env(&mut self, key: impl AsRef<OsStr>, val: impl AsRef<OsStr>) -> &mut Self {
        self.0.env(key, val);
        self
    }

    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.0.envs(vars);
        self
    }

    pub fn env_remove(&mut self, key: impl AsRef<OsStr>) -> &mut Self {
        self.0.env_remove(key);
        self
    }

    pub fn env_clear(&mut self) -> &mut Self {
        self.0.env_clear();
        self
    }

    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.0.current_dir(dir);
        self
    }

    pub fn stdin(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stdin(cfg.into());
        self
    }

    pub fn stdout(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stdout(cfg.into());
        self
    }

    pub fn stderr(&mut self, cfg: impl Into<Stdio>) -> &mut Self {
        self.0.stderr(cfg.into());
        self
    }

    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Self {
        self.0.kill_on_drop(kill_on_drop);
        self
    }

    pub fn spawn(&mut self) -> std::io::Result<Child> {
        self.0.spawn()
    }

    pub async fn output(&mut self) -> std::io::Result<std::process::Output> {
        #[cfg(target_os = "windows")]
        {
            self.create_process_on_blocking_thread(|command| command.output())
                .await
                .await
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.0.output().await
        }
    }

    pub async fn status(&mut self) -> std::io::Result<std::process::ExitStatus> {
        #[cfg(target_os = "windows")]
        {
            self.create_process_on_blocking_thread(|command| command.status())
                .await
                .await
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.0.status().await
        }
    }

    /// Runs `create`, which is expected to create the process, on a blocking
    /// thread and returns what it produced.
    ///
    /// Creating a process on Windows can take seconds: antivirus may scan the
    /// executable first, and std serializes every `CreateProcessW` call in the
    /// process behind one lock, so a single slow launch stalls all the others.
    /// Callers are often on the main thread, which must not wait for that.
    #[cfg(target_os = "windows")]
    async fn create_process_on_blocking_thread<T: Send + 'static>(
        &mut self,
        create: impl FnOnce(&mut smol::process::Command) -> T + Send + 'static,
    ) -> T {
        // The command has to move to the other thread. If this future is
        // dropped before it comes back, `self` keeps the inert placeholder.
        let mut command = std::mem::replace(&mut self.0, smol::process::Command::new(""));
        let (command, created) = smol::unblock(move || {
            let created = create(&mut command);
            (command, created)
        })
        .await;
        self.0 = command;
        created
    }

    pub fn get_program(&self) -> &OsStr {
        self.0.get_program()
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;

    #[test]
    fn test_output_collects_stdout_and_exit_code() {
        let mut command = new_command("cmd.exe");
        command.args(["/c", "echo hello && exit 3"]);

        let output = smol::block_on(command.output()).expect("failed to run cmd.exe");

        assert_eq!(output.status.code(), Some(3));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    }

    #[test]
    fn test_command_can_run_again_after_returning_from_the_blocking_thread() {
        let mut command = new_command("cmd.exe");
        command.args(["/c", "exit 4"]);

        let status = smol::block_on(command.status()).expect("failed to run cmd.exe");
        assert_eq!(status.code(), Some(4));

        let output = smol::block_on(command.output()).expect("failed to run cmd.exe again");
        assert_eq!(output.status.code(), Some(4));
        assert_eq!(command.get_program(), OsStr::new("cmd.exe"));
    }

    #[test]
    fn test_output_reports_a_missing_program() {
        let mut command = new_command("zed-test-program-that-does-not-exist.exe");

        let error = smol::block_on(command.output()).expect_err("spawn should fail");

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}
