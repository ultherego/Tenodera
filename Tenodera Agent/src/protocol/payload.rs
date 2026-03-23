#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Payload {
    SystemInfo,
    HardwareInfo,
    TopProcesses,
    DiskUsage,
    NetworkStats,
    JournalQuery,
    FileList,
    SuperuserVerify,
    SystemdUnits,
    MetricsStream,
    StorageStream,
    NetworkingStream,
    TerminalPty,
    SystemdManage,
    ContainerManage,
    NetworkingManage,
    PackagesManage,
    KdumpInfo,
    LogFiles,
    Custom(String),
}

impl Payload {
    pub fn from_str(s: &str) -> Self {
        match s {
            "system.info" => Self::SystemInfo,
            "hardware.info" => Self::HardwareInfo,
            "top.processes" => Self::TopProcesses,
            "disk.usage" => Self::DiskUsage,
            "network.stats" => Self::NetworkStats,
            "journal.query" => Self::JournalQuery,
            "file.list" => Self::FileList,
            "superuser.verify" => Self::SuperuserVerify,
            "systemd.units" => Self::SystemdUnits,
            "metrics.stream" => Self::MetricsStream,
            "storage.stream" => Self::StorageStream,
            "networking.stream" => Self::NetworkingStream,
            "terminal.pty" => Self::TerminalPty,
            "systemd.manage" => Self::SystemdManage,
            "container.manage" => Self::ContainerManage,
            "networking.manage" => Self::NetworkingManage,
            "packages.manage" => Self::PackagesManage,
            "kdump.info" => Self::KdumpInfo,
            "log.files" => Self::LogFiles,
            other => Self::Custom(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::SystemInfo => "system.info",
            Self::HardwareInfo => "hardware.info",
            Self::TopProcesses => "top.processes",
            Self::DiskUsage => "disk.usage",
            Self::NetworkStats => "network.stats",
            Self::JournalQuery => "journal.query",
            Self::FileList => "file.list",
            Self::SuperuserVerify => "superuser.verify",
            Self::SystemdUnits => "systemd.units",
            Self::MetricsStream => "metrics.stream",
            Self::StorageStream => "storage.stream",
            Self::NetworkingStream => "networking.stream",
            Self::TerminalPty => "terminal.pty",
            Self::SystemdManage => "systemd.manage",
            Self::ContainerManage => "container.manage",
            Self::NetworkingManage => "networking.manage",
            Self::PackagesManage => "packages.manage",
            Self::KdumpInfo => "kdump.info",
            Self::LogFiles => "log.files",
            Self::Custom(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
