mod init;

use std::process::Command;

use anyhow::Result;
use serde::{Serialize, Deserialize};

fn main() {
    env_logger::init();

    let s = std::fs::read_to_string("test.toml").unwrap();
    let mut t: std::collections::HashMap<String, Container> = toml::from_str(&s).unwrap();
    t.get_mut("c-devel").unwrap().name = "c-devel-t".to_owned();
    println!("{t:#?}");
    t["c-devel"].create_command();
}

/// Get the absolute path of our own executable (using the `/proc/self/exe` symlink.)
fn self_path() -> Result<String> {
    std::fs::read_link("/proc/self/exe")
        .map(|p| p.to_string_lossy().into_owned() )
        .map_err(Into::into)
}

/// Top-level container assembly manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    #[serde(default)]
    pub name            : String,
    pub image           : String,
    pub pull            : Option<String>,
    #[serde(default)]
    /// Array of additional arguments to pass to `podman create`.
    pub additional_args : Vec<String>,
    pub init            : ContainerInit,
    pub share           : ContainerShare,
}

impl Container {
    pub fn create_command(&self) -> Result<Command> {
        let mut cmd = Command::new("podman");
    
        cmd
            .arg("create")
            .args(&self.additional_args)
            .args(["--hostname", &self.name])
            .args(["--name", &self.name])
            .args(["--label", "\"manager=pocket-dimension\""])
            // The entrypoint subcommand expects to be run as root.
            .args(["--user", "root:root"])
            .args( self.net_arg() )
            .args( self.ipc_arg() )
            .args( self.sec_args() )
            .args( self.mnt_args() )
            // We do one unconditional RO bind mount to inject our own executable into the container,
            // so it can later be invoked as the entrypoint.
            .args([
                "--mount",
                &format!("type=bind,source={},dst=/usr/bin/entrypoint,ro=true", self_path()?)
            ])
            .args(["--entrypoint", "/usr/bin/entrypoint"])
            .args([
                &self.image,
                "--username",  &whoami::username(),
                "--uid",       unsafe { &libc::getuid().to_string() },
                "--gid",       unsafe { &libc::getgid().to_string() },
                "--pre-init",  &serde_json::to_string(&self.init.pre_init)?,
                "--init",      &serde_json::to_string(&self.init.init)?,
                "--packages",  &serde_json::to_string(&self.init.packages)?,
            ]);
    
        if let Some(c) = &self.init.package_command {
            cmd.args(["--package-command", c]);
        }
        
        println!("{:#?}", cmd);
    
        Ok(cmd)
    }

    pub fn net_arg(&self) -> [&str; 2] {
        [
            "--network",
            self.share.net.as_deref().unwrap_or("\"\"")
        ]
    }

    pub fn ipc_arg(&self) -> [&str; 2] {
        [
            "--ipc",
            self.share.ipc.as_deref().unwrap_or("\"\"")
        ]  
    }

    pub fn sec_args(&self) -> Vec<&str> {
        let mut out = Vec::new();

        for option in &self.share.sec {
            out.extend(["--security-opt", &option])
        }

        out
    }

    pub fn mnt_args(&self) -> Vec<String> {
        use std::fmt::Write;

        let mut out = Vec::new();

        for mount in &self.share.mounts {
            let mut arg = format!("type={},", mount.ty);
    
            if let Some(src) = &mount.src {
                let _ = write!(arg, "src={src},");
            }
    
            let _ = write!(arg, "dst={},", mount.dst);
    
            for opt in &mount.opts {
                let _ = write!(arg, "{opt},");
            }

            // Obliterate trailing comma
            arg.truncate(arg.len() - 1);
    
            out.extend(["--mount".to_owned(), arg])
        }

        out
    }
}

/// Container initialization options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInit {
    #[serde(default)]
    /// Array of shell commands to execute *in the container* at the start of container init.
    pub pre_init        : Vec<String>,
    #[serde(default)]
    /// Array of shell commands to execute *in the container* at the end of container init.
    pub init            : Vec<String>,
    #[serde(default)]
    /// Array of package names to install during container init.
    pub packages        : Vec<String>,
    /// Optional template command for images with unrecognized package managers.
    /// 
    /// This template should contain the special string `%packages%`, which will be replaced
    /// with the items in `packages` (delimited by spaces.)
    pub package_command : Option<String>,
}

/// Container host integration options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerShare {
    /// Container network mode. 
    /// 
    /// - Corresponds to the `--network` `podman create` argument. 
    /// - Defaults to `private.`
    pub net    : Option<String>,
    /// Container IPC namespace mode.
    /// 
    /// - Corresponds to the `--ipc` `podman create` argument.
    /// - The default depends on the user's `containers.conf`.
    pub ipc    : Option<String>,
    #[serde(default)]
    /// Container security options.
    /// 
    /// - Corresponds to the `--security-opt` `podman create` argument.
    /// - No defaults; security options are strictly additive.
    pub sec    : Vec<String>,
    #[serde(default)]
    /// Shorthand for bind-mounting the host's `/dev` and `/sys` into the container.
    pub dev    : bool,
    #[serde(default)]
    /// Shorthand for bind-mounting the host's `/mnt` and `/media` into the container.
    pub mnt    : bool,
    #[serde(default)]
    /// Array of additional arbitrary filesystem mounts.
    pub mounts : Vec<ContainerMount>
}

/// Options for a container filesystem mount, such a bind or volume.
/// 
/// Corresponds to the `--mount` `podman create` argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMount {
    #[serde(rename = "type")]
    /// The mount type.
    pub ty   : String,
    /// The mount source path or volume, if any.
    pub src  : Option<String>,
    /// The destination path for the mount inside the container.
    pub dst  : String,
    #[serde(default)]
    /// Optional options for the mount.
    pub opts : Vec<String>,
}

