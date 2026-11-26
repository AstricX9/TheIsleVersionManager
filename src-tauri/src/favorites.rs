use crate::library::Result;
use serde::Serialize;
use std::{
    env,
    net::{SocketAddrV4, UdpSocket},
    os::windows::process::CommandExt,
    process::Command,
    time::Duration,
};

const INFO: &[u8] = b"\xff\xff\xff\xffTSource Engine Query\0";

#[derive(Serialize)]
pub struct ServerInfo {
    map: String,
    description: String,
    game_port: Option<u16>,
}

fn field<'a>(data: &mut &'a [u8]) -> Option<&'a [u8]> {
    let end = data.iter().position(|&b| b == 0)?;
    let (value, rest) = data.split_at(end);
    *data = &rest[1..];
    Some(value)
}

fn parse_info(packet: &[u8]) -> Option<ServerInfo> {
    let mut data = packet.strip_prefix(b"\xff\xff\xff\xffI")?.get(1..)?;
    field(&mut data)?; // server name
    let map = String::from_utf8_lossy(field(&mut data)?).into_owned();
    let folder = field(&mut data)?;
    if !folder.eq_ignore_ascii_case(b"theisle") {
        return None;
    }
    let description = String::from_utf8_lossy(field(&mut data)?).into_owned();
    data = data.get(9..)?; // app ID, player counts, server type, OS, visibility and VAC
    field(&mut data)?; // protocol's version string
    let game_port = match data.first() {
        Some(flags) if flags & 0x80 != 0 => {
            Some(u16::from_le_bytes(data.get(1..3)?.try_into().ok()?)).filter(|p| *p != 0)
        }
        _ => None,
    };
    Some(ServerInfo {
        map,
        description,
        game_port,
    })
}

fn endpoint(value: &str) -> Result<SocketAddrV4> {
    value
        .parse::<SocketAddrV4>()
        .ok()
        .filter(|a| {
            a.port() != 0
                && !a.ip().is_unspecified()
                && !a.ip().is_multicast()
                && !a.ip().is_broadcast()
        })
        .ok_or_else(|| {
            "Enter a server IPv4 address and query port, such as 127.0.0.1:27015.".into()
        })
}

fn query(address: SocketAddrV4) -> Result<ServerInfo> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .connect(address)
        .and_then(|_| socket.set_read_timeout(Some(Duration::from_secs(2))))
        .and_then(|_| socket.send(INFO))
        .map_err(|e| e.to_string())?;
    let mut packet = [0u8; 4096];
    let recv = |buf: &mut [u8]| {
        socket.recv(buf).map_err(|_| format!("No Steam query reply from {address}. Check that the server is running and the query port is correct."))
    };
    let mut size = recv(&mut packet)?;
    if size == 9 && packet.starts_with(b"\xff\xff\xff\xffA") {
        socket
            .send(&[INFO, &packet[5..9]].concat())
            .map_err(|e| e.to_string())?;
        size = recv(&mut packet)?;
    }
    parse_info(&packet[..size])
        .ok_or_else(|| "The endpoint did not return a recognized The Isle server reply.".into())
}

#[tauri::command]
pub async fn check_isle_server(address: String) -> Result<ServerInfo> {
    let address = endpoint(&address)?;
    tauri::async_runtime::spawn_blocking(move || query(address))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn add_steam_favorite(address: String, game_port: u16) -> Result<()> {
    let address = endpoint(&address)?;
    if game_port == 0 {
        return Err("Enter the game port.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let launcher = env::current_exe().map_err(|e| e.to_string())?;
        let folder = launcher.parent().ok_or("Cannot find launcher folder.")?;
        let helper = folder.join("tools/isle-steam-favorite.exe");
        let output = Command::new(helper)
            .args(["add", &address.to_string(), &game_port.to_string()])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Cannot open the Steam Favorites helper: {e}"))?;
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|s| s.trim() == "saved")
        {
            Ok(())
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(if error.trim().is_empty() {
                "Steam Favorites is unavailable. Check that steam_api64.dll is in the tools folder."
                    .into()
            } else {
                error.trim().into()
            })
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_two_distinct_ports_and_handles_short_replies() {
        let response = b"\xff\xff\xff\xffI\x11Name\0Isla_Spiro\0theisle\x000.5.12.08\0\0\0\0\x32\0dw\0\x01v1\0\x80\x61\x1e";
        let info = parse_info(response).unwrap();
        assert_eq!(info.game_port, Some(7777));
        assert_eq!(info.description, "0.5.12.08");
        assert!(parse_info(&response[..response.len() - 1]).is_none());
        assert_eq!(
            parse_info(&response[..response.len() - 3])
                .unwrap()
                .game_port,
            None
        );
        assert!(endpoint("127.0.0.1:0").is_err());
    }
}
