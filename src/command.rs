use anyhow::{Context, Result, bail};
use std::{
    fs::File,
    io::Read,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Control(Arc<AtomicBool>);
impl Control {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn check(&self) -> Result<()> {
        if self.0.load(Ordering::Relaxed) {
            bail!("Operation cancelled")
        }
        Ok(())
    }
}

pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: String,
}
impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
    pub fn success(self) -> Result<Self> {
        if self.code != 0 {
            bail!("{}\n{}\n(exit {})", self.text(), self.stderr, self.code)
        }
        Ok(self)
    }
}

/// Argument arrays only. Output goes to owned temp files, avoiding pipe deadlocks.
pub fn run(
    executable: &str,
    args: &[&str],
    timeout: Duration,
    control: &Control,
) -> Result<Output> {
    control.check()?;
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = Command::new(executable)
        .args(args)
        .current_dir("/")
        .env("LC_ALL", "C")
        .env("LANG", "en_US.UTF-8")
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()
        .with_context(|| format!("Cannot launch {executable}"))?;
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
        let check = control.check().and_then(|()| {
            if start.elapsed() >= timeout {
                bail!("Timed out: {executable}")
            } else {
                Ok(())
            }
        });
        if let Err(error) = check {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        thread::sleep(Duration::from_millis(100));
    };
    // cloned descriptors share offsets; explicitly rewind before reading.
    use std::io::{Seek, SeekFrom};
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut output = Vec::new();
    stdout.take(64 * 1024 * 1024).read_to_end(&mut output)?;
    let mut errors = String::new();
    stderr.take(4 * 1024 * 1024).read_to_string(&mut errors)?;
    Ok(Output {
        code: status.code().unwrap_or(-1),
        stdout: output,
        stderr: errors,
    })
}

pub fn quick(executable: &str, args: &[&str]) -> Result<Output> {
    run(
        executable,
        args,
        Duration::from_secs(40),
        &Control::default(),
    )
}

pub fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}
