//! Run the system Wi-Fi service's asynchronous scan and connect operations.
use agx_net::{AccessPoint, Reply, Request, Security, Status};
use std::{
    thread,
    time::{Duration, Instant},
};

pub(super) fn security(index: i32) -> Security {
    match index {
        0 => Security::Open,
        1 => Security::Wpa2,
        2 => Security::Wpa3,
        3 => Security::Owe,
        _ => Security::Unsupported,
    }
}

pub(super) fn security_index(security: Security) -> i32 {
    match security {
        Security::Open => 0,
        Security::Wpa2 => 1,
        Security::Wpa3 => 2,
        Security::Owe => 3,
        Security::Unsupported => -1,
    }
}

fn call(request: &Request) -> Result<Reply, String> {
    let reply = agx_net::request(request).map_err(|error| error.to_string())?;
    if reply.ok {
        Ok(reply)
    } else {
        Err(reply.message)
    }
}

fn wait(
    timeout: Duration,
    ready: impl Fn(&Status) -> bool,
    timeout_message: &str,
) -> Result<Status, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = call(&Request::Status)?.status;
        if ready(&status) {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(timeout_message.into());
        }
        thread::sleep(Duration::from_millis(350));
    }
}

pub(super) fn scan(country: String) -> Result<Vec<AccessPoint>, String> {
    call(&Request::Country {
        country: country.trim().into(),
    })?;
    call(&Request::Scan)?;
    let status = wait(
        Duration::from_secs(20),
        |status| !status.scanning,
        "Wi-Fi scan timed out. Try again.",
    )?;
    let mut rows = status.access_points;
    rows.retain(|row| row.security != Security::Unsupported);
    rows.sort_by(|a, b| b.signal_dbm.cmp(&a.signal_dbm));
    let mut unique = Vec::<AccessPoint>::new();
    for row in rows {
        if !unique
            .iter()
            .any(|point| point.ssid_hex == row.ssid_hex && point.security == row.security)
        {
            unique.push(row);
        }
    }
    Ok(unique)
}

pub(super) fn connect(
    country: String,
    ssid_hex: String,
    request: Request,
) -> Result<String, String> {
    call(&Request::Country {
        country: country.trim().into(),
    })?;
    call(&request)?;
    drop(request);
    let status = wait(
        Duration::from_secs(65),
        |status| {
            status.error.is_some()
                || (status.phase == "Connected"
                    && status
                        .saved
                        .iter()
                        .any(|row| row.current && row.ssid_hex == ssid_hex))
        },
        "This computer did not obtain a Wi-Fi address. Check the network settings and try again.",
    )?;
    if let Some(error) = status.error {
        Err(error)
    } else {
        Ok(status.ssid.unwrap_or_else(|| "Wi-Fi".into()))
    }
}

pub(super) fn reconnect() -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(reply) = call(&Request::Status) {
            if reply.status.phase == "Connected" {
                return reply.status.ssid;
            }
            if reply.status.saved.is_empty() {
                return None;
            }
        }
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(350));
    }
}
