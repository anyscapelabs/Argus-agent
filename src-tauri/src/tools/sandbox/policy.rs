use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Restricted,
    Project,
    Host,
}

impl Profile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Profile::Restricted => "restricted",
            Profile::Project => "project",
            Profile::Host => "host",
        }
    }

    pub fn parse(s: &str) -> Option<Profile> {
        match s.trim().to_lowercase().as_str() {
            "restricted" => Some(Profile::Restricted),
            "project" => Some(Profile::Project),
            "host" => Some(Profile::Host),
            _ => None,
        }
    }

    pub fn is_isolated(&self) -> bool {
        !matches!(self, Profile::Host)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsAccess {
    Read,
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FsPolicy {
    Unrestricted,
    Scoped(Vec<FsRule>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsRule {
    pub path: PathBuf,
    pub access: FsAccess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetPolicy {
    None,
    Ports(Vec<u16>),
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Limits {
    pub wall_secs: Option<u64>,
    pub output_bytes: Option<usize>,
    pub memory_bytes: Option<u64>,
    pub procs: Option<u32>,
    pub cpu_secs: Option<u64>,
    pub nofile: Option<u32>,
    pub fsize_bytes: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvPolicy {
    Only(Vec<String>),
    Inherit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub profile: Profile,
    pub fs: FsPolicy,
    pub net: NetPolicy,
    pub limits: Limits,
    pub env: EnvPolicy,
    pub cwd: PathBuf,
}

pub struct PolicyCtx<'a> {
    pub project: &'a Path,
    pub tmp: &'a Path,
    pub home: &'a Path,
}

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;

const SYSROOT: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/lib32", "/libx32", "/opt",
];

/// A shell cannot start without these: `/dev/null` for the stdio it was handed,
/// `/proc/self/*` for its own pid and page size, `/sys` for CPU and device facts.
const RUNTIME: &[&str] = &["/dev", "/proc", "/sys", "/run"];

const LOADER_CONF: &[&str] = &[
    "/etc/ld.so.cache",
    "/etc/ld.so.conf",
    "/etc/ld.so.conf.d",
    "/etc/localtime",
    "/etc/ssl/certs",
];

const DEP_CACHES: &[&str] = &[".cargo", ".rustup", ".bun", ".npm", ".local/share/pnpm"];

const ENV_RESTRICTED: &[&str] = &["PATH", "HOME", "LANG", "LC_ALL", "TERM", "TMPDIR", "USER"];

pub fn resolve(profile: Profile, ctx: &PolicyCtx) -> Policy {
    match profile {
        Profile::Host => Policy {
            profile,
            fs: FsPolicy::Unrestricted,
            net: NetPolicy::Full,
            limits: Limits {
                output_bytes: Some(8 * MIB as usize),
                ..Limits::default()
            },
            env: EnvPolicy::Inherit,
            cwd: ctx.project.to_path_buf(),
        },
        Profile::Restricted => {
            let mut fs = sysroot_rules();

            for p in LOADER_CONF {
                fs.push(FsRule {
                    path: PathBuf::from(p),
                    access: FsAccess::Read,
                });
            }

            fs.push(FsRule {
                path: ctx.tmp.to_path_buf(),
                access: FsAccess::Write,
            });

            Policy {
                profile,
                fs: FsPolicy::Scoped(fs),
                net: NetPolicy::None,
                limits: Limits {
                    wall_secs: Some(120),
                    output_bytes: Some(64 * KIB as usize),
                    memory_bytes: Some(2 * 1024 * MIB),
                    procs: Some(64),
                    cpu_secs: Some(120),
                    nofile: Some(256),
                    fsize_bytes: Some(256 * MIB),
                },
                env: EnvPolicy::Only(ENV_RESTRICTED.iter().map(|s| (*s).into()).collect()),
                cwd: ctx.tmp.to_path_buf(),
            }
        }
        Profile::Project => {
            let mut fs = sysroot_rules();

            for p in LOADER_CONF {
                fs.push(FsRule {
                    path: PathBuf::from(p),
                    access: FsAccess::Read,
                });
            }

            // Read-only: a build must not be able to poison a shared cache.
            for c in DEP_CACHES {
                fs.push(FsRule {
                    path: ctx.home.join(c),
                    access: FsAccess::Read,
                });
            }

            fs.push(FsRule {
                path: ctx.home.join(".cache"),
                access: FsAccess::Write,
            });
            fs.push(FsRule {
                path: ctx.project.to_path_buf(),
                access: FsAccess::Write,
            });
            fs.push(FsRule {
                path: ctx.tmp.to_path_buf(),
                access: FsAccess::Write,
            });

            Policy {
                profile,
                fs: FsPolicy::Scoped(fs),
                net: NetPolicy::Ports(vec![80, 443]),
                limits: Limits {
                    wall_secs: Some(900),
                    output_bytes: Some(256 * KIB as usize),
                    memory_bytes: Some(4 * 1024 * MIB),
                    procs: Some(256),
                    cpu_secs: Some(900),
                    nofile: Some(1024),
                    fsize_bytes: Some(1024 * MIB),
                },
                env: EnvPolicy::Inherit,
                cwd: ctx.project.to_path_buf(),
            }
        }
    }
}

fn sysroot_rules() -> Vec<FsRule> {
    SYSROOT
        .iter()
        .chain(RUNTIME.iter())
        .map(|p| FsRule {
            path: PathBuf::from(p),
            access: FsAccess::Read,
        })
        .collect()
}
