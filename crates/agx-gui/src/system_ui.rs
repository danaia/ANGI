//! System snapshot presentation, units, clock and optional preview palette.
use crate::{AppWindow, HwRow, NetRow, Sys};
use agx_core::Snapshot;
use slint::{ComponentHandle, ModelRc, VecModel};
pub(super) fn apply(ui: &AppWindow, s: &Snapshot, temperature: Option<f64>) {
    let sys = ui.global::<Sys>();
    sys.set_hostname(s.hostname.clone().into());
    sys.set_board(s.board.clone().into());
    sys.set_kernel(s.kernel.clone().into());
    sys.set_uptime(duration(s.uptime_secs).into());
    sys.set_cpu_percent(s.cpu_usage_percent.unwrap_or(0.0) as f32);
    sys.set_cpu_detail(format!("{} cores", s.cpu_count).into());
    let temperature = temperature
        .map(|value| format!("{value:.1} °C"))
        .unwrap_or_else(|| "Unavailable".into());
    sys.set_cpu_temperature(temperature.into());
    sys.set_load_text(
        s.load_average
            .iter()
            .map(|v| format!("{v:.2}"))
            .collect::<Vec<_>>()
            .join("  ")
            .into(),
    );
    let mem_used = s
        .memory
        .total_bytes
        .saturating_sub(s.memory.available_bytes);
    sys.set_mem_percent(percent(mem_used, s.memory.total_bytes));
    sys.set_mem_detail(format!("{} / {}", bytes(mem_used), bytes(s.memory.total_bytes)).into());
    match &s.root_storage {
        Some(d) => {
            let used = d.total_bytes.saturating_sub(d.available_bytes);
            sys.set_disk_percent(percent(used, d.total_bytes));
            sys.set_disk_detail(format!("{} / {}", bytes(used), bytes(d.total_bytes)).into());
        }
        None => sys.set_disk_detail("unavailable".into()),
    }
    let primary = s
        .networks
        .iter()
        .filter(|n| n.name != "lo")
        .find_map(|n| n.ipv4.first().cloned())
        .unwrap_or_else(|| "no network".into());
    sys.set_primary_ip(primary.into());
    let nets: Vec<NetRow> = s
        .networks
        .iter()
        .map(|n| NetRow {
            name: n.name.clone().into(),
            state: n.state.clone().into(),
            mac: n.mac.clone().into(),
            ipv4: if n.ipv4.is_empty() {
                "-".into()
            } else {
                n.ipv4.join(", ").into()
            },
        })
        .collect();
    sys.set_networks(ModelRc::new(VecModel::from(nets)));
    let hw: Vec<HwRow> = s
        .hardware
        .iter()
        .map(|h| HwRow {
            kind: h.kind.clone().into(),
            name: h.name.clone().into(),
            state: h.state.clone().into(),
            detail: h.detail.clone().into(),
        })
        .collect();
    sys.set_hardware(ModelRc::new(VecModel::from(hw)));
    sys.set_warnings(s.warnings.join("; ").into());
}

pub(super) fn apply_network_safety(sys: &Sys, profile: crate::network_safety::Profile) {
    sys.set_network_risk_score(profile.score);
    sys.set_network_risk_reason(profile.reason.into());
}

fn percent(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 * 100.0 / whole as f64) as f32
    }
}

pub(super) fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3600, secs % 3600 / 60);
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m {}s", secs % 60)
    }
}

/// Short date for the top bar, such as "Sat 21 Nov".
pub(super) fn local_date() -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    // SAFETY: localtime_r writes only into the zeroed tm we pass.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        let day = DAYS.get(tm.tm_wday as usize).copied().unwrap_or("");
        let month = MONTHS.get(tm.tm_mon as usize).copied().unwrap_or("");
        format!("{day} {} {month}", tm.tm_mday)
    }
}

pub(super) fn local_time() -> String {
    // SAFETY: localtime_r writes only into the zeroed tm we pass.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&now, &mut tm).is_null() {
            return String::new();
        }
        format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
    }
}
