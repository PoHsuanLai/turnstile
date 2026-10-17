//! The facts of a caller's unit, read from the unit files the way the unit manager would find
//! them, for a service that has no manager to ask.
//!
//! A root service cannot ask a user's manager (that is a socket the user controls) and links no
//! bus, so `peephole`'s `Full` proof cannot come from a manager call here. It comes from what a
//! root service can read with the one capability it keeps:
//!
//! - the unit's fragment is a file in a root-owned unit folder, with every drop-in root-owned, and
//!   the unit is not transient;
//! - the unit's `UnsetEnvironment=` holds all the loader variables, and the process's own
//!   environment holds none of them;
//! - nothing is tracing the process;
//! - the unit's `ExecStart=` program is the row's.
//!
//! What is not shown is the manager's `MainPID`: for a system unit the control group can only be
//! filled by the manager, for a user unit it can be filled by the user, which the documentation
//! names as the residual of a root service without a manager (see ARCHITECTURE.md).
//!
//! The folders read are the root-owned ones only. A user's own unit folders sit under a home the
//! service cannot read (it holds `CAP_SYS_PTRACE` and no file-reading capability), so a user's
//! override of the unit is not seen; the process checks above are what covers its effect.

mod directives;
mod process;

use directives::ServiceSettings;
use peephole::{ProcFs, dirs_trust, weaker};
use peephole_core::{
    CgroupPath, DropIns, ExePath, FileTrust, Fragment, Loader, Pid, Transient, UnitFacts, UnitName,
};
pub use process::ProcessFault;
use std::io;
use std::path::{Path, PathBuf};

/// The largest unit file or drop-in read.
pub const UNIT_FILE_CAP: u64 = 64 * 1024;
/// The most drop-in files read for one unit; more is refused.
pub const DROP_IN_MAX: usize = 32;

/// Folders a system manager reads units from, highest precedence first; all root-owned.
const SYSTEM_DIRS: [&str; 9] = [
    "/etc/systemd/system.control",
    "/run/systemd/system.control",
    "/etc/systemd/system",
    "/run/systemd/system",
    "/run/systemd/generator",
    "/usr/local/lib/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
    "/run/systemd/generator.late",
];
/// Folders a user manager reads units from that are not a person's own, highest precedence first.
const USER_DIRS: [&str; 7] = [
    "/etc/systemd/user",
    "/etc/xdg/systemd/user",
    "/run/systemd/user",
    "/run/systemd/generator.user",
    "/usr/local/lib/systemd/user",
    "/usr/lib/systemd/user",
    "/lib/systemd/user",
];
/// Where the system manager keeps units made at run time.
const TRANSIENT_DIR: &str = "/run/systemd/transient";

/// Why a caller could not be shown to be the real unit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NotUnit {
    /// The control group's last part is not a service.
    #[error("not in a service's control group")]
    NoUnit,
    /// The control group is under neither the system's slice nor a user's manager.
    #[error("not in a place a manager puts services")]
    Place,
    /// The process itself shows a fault.
    #[error("{0}")]
    Process(#[from] ProcessFault),
    /// A unit file could not be read.
    #[error("a unit file could not be read: {0}")]
    Read(String),
}

/// Which manager the control group belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Manager {
    System,
    User,
}

impl Manager {
    fn of(cgroup: &CgroupPath) -> Option<Self> {
        match cgroup.segments().next()? {
            "system.slice" => Some(Self::System),
            "user.slice" => Some(Self::User),
            _ => None,
        }
    }

    fn dirs(self) -> &'static [&'static str] {
        match self {
            Self::System => &SYSTEM_DIRS,
            Self::User => &USER_DIRS,
        }
    }
}

/// The unit facts for the process `pid` in `cgroup`, read through `fs`.
pub fn facts<P: ProcFs>(fs: &P, pid: Pid, cgroup: &CgroupPath) -> Result<UnitFacts, NotUnit> {
    let name = cgroup.unit_leaf().ok_or(NotUnit::NoUnit)?;
    if !name.as_str().ends_with(".service") {
        return Err(NotUnit::NoUnit);
    }
    let manager = Manager::of(cgroup).ok_or(NotUnit::Place)?;
    process::check(fs, pid)?;
    let files = UnitText::find(fs, manager, &name)?;
    let settings = files.settings(fs)?;
    Ok(UnitFacts {
        main_pid: Some(pid),
        fragment: files.fragment,
        drop_ins: files.drop_ins,
        transient: transient(fs, manager, &name)?,
        exec: settings.exec.and_then(|text| ExePath::new(&text).ok()),
        loader: Loader::from_unset(&settings.unset),
        name,
    })
}

/// The files that make up one unit.
struct UnitText {
    fragment: Fragment,
    drop_ins: DropIns,
    /// The fragment and the drop-ins that apply, in the order systemd folds them.
    order: Vec<PathBuf>,
}

impl UnitText {
    fn find<P: ProcFs>(fs: &P, manager: Manager, name: &UnitName) -> Result<Self, NotUnit> {
        let mut fragment = Fragment::Missing;
        let mut order = Vec::new();
        for dir in manager.dirs() {
            let path = Path::new(dir).join(name.as_str());
            if let Some(trust) = trust_if_present(fs, &path)? {
                if matches!(fragment, Fragment::Missing) {
                    fragment = match trust {
                        FileTrust::RootOnly => Fragment::RootOwned(path.clone()),
                        _ => Fragment::Writable(path.clone()),
                    };
                    order.push(path);
                }
            }
        }
        let (drop_ins, mut files) = drop_in_files(fs, manager, name)?;
        files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        files.dedup_by(|a, b| a.file_name() == b.file_name());
        order.extend(files);
        Ok(Self {
            fragment,
            drop_ins,
            order,
        })
    }

    fn settings<P: ProcFs>(&self, fs: &P) -> Result<ServiceSettings, NotUnit> {
        self.order
            .iter()
            .try_fold(ServiceSettings::default(), |acc, path| {
                let bytes = fs
                    .read_file_capped(path, UNIT_FILE_CAP)
                    .map_err(|e| read_fault(path, &e))?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| NotUnit::Read(format!("{} is not text", path.display())))?;
                Ok(acc.fold(&text))
            })
    }
}

/// The drop-in files of `name` in every folder, with their combined trust. The highest-precedence
/// folder comes first, so a file name present twice keeps the first when deduplicated.
fn drop_in_files<P: ProcFs>(
    fs: &P,
    manager: Manager,
    name: &UnitName,
) -> Result<(DropIns, Vec<PathBuf>), NotUnit> {
    let mut summary = DropIns::None;
    let mut files = Vec::new();
    for dir in manager.dirs() {
        for folder in [format!("{}.d", name.as_str()), "service.d".to_owned()] {
            for path in conf_files(fs, &Path::new(dir).join(folder))? {
                let trust = trust_of_file(fs, &path)?;
                summary = match (summary, trust) {
                    (DropIns::Writable, _) | (_, FileTrust::Writable(_)) => DropIns::Writable,
                    _ => DropIns::RootOwned,
                };
                files.push(path);
            }
        }
    }
    if files.len() > DROP_IN_MAX {
        return Err(NotUnit::Read("too many drop-in files".to_owned()));
    }
    Ok((summary, files))
}

/// The `*.conf` files directly in `folder`; none when it does not exist.
fn conf_files<P: ProcFs>(fs: &P, folder: &Path) -> Result<Vec<PathBuf>, NotUnit> {
    match fs.list_dir(folder) {
        Ok(mut files) => {
            files.retain(|p| p.extension().is_some_and(|e| e == "conf"));
            Ok(files)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(read_fault(folder, &e)),
    }
}

/// The trust of `path` and every folder above it; `None` when there is no such file.
fn trust_if_present<P: ProcFs>(fs: &P, path: &Path) -> Result<Option<FileTrust>, NotUnit> {
    match fs.trust_of(path) {
        Ok(file) => {
            let above = dirs_trust(fs, path).map_err(|e| read_fault(path, &e))?;
            Ok(Some(weaker(above, file)))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(read_fault(path, &e)),
    }
}

fn trust_of_file<P: ProcFs>(fs: &P, path: &Path) -> Result<FileTrust, NotUnit> {
    trust_if_present(fs, path)?.ok_or_else(|| NotUnit::Read(format!("{} vanished", path.display())))
}

/// Whether the system manager has a run-time unit of this name; a user's run-time units sit in a
/// folder the service cannot read, and the process checks stand for them.
fn transient<P: ProcFs>(fs: &P, manager: Manager, name: &UnitName) -> Result<Transient, NotUnit> {
    if manager == Manager::User {
        return Ok(Transient::No);
    }
    let path = Path::new(TRANSIENT_DIR).join(name.as_str());
    Ok(match trust_if_present(fs, &path)? {
        Some(_) => Transient::Yes,
        None => Transient::No,
    })
}

fn read_fault(path: &Path, error: &io::Error) -> NotUnit {
    NotUnit::Read(format!("{}: {}", path.display(), error.kind()))
}
