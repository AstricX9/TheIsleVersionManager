use std::{
    env,
    net::SocketAddrV4,
    time::{SystemTime, UNIX_EPOCH},
};
use steamworks_sys as steam;

const APP: u32 = 376210;
const FAVORITE: u32 = 1;

fn is_saved(mm: *mut steam::ISteamMatchmaking, ip: u32, game: u16, query: u16) -> bool {
    // The interface remains valid between SteamAPI_InitFlat and SteamAPI_Shutdown.
    unsafe {
        (0..steam::SteamAPI_ISteamMatchmaking_GetFavoriteGameCount(mm)).any(|i| {
            let (mut app, mut addr, mut conn, mut port, mut flags, mut last) = (0, 0, 0, 0, 0, 0);
            steam::SteamAPI_ISteamMatchmaking_GetFavoriteGame(
                mm, i, &mut app, &mut addr, &mut conn, &mut port, &mut flags, &mut last,
            ) && app == APP
                && addr == ip
                && conn == game
                && port == query
                && flags & FAVORITE != 0
        })
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    let [mode, address, game_port] = args.as_slice() else {
        return Err("Expected check/add, IPv4:query-port and game port.".into());
    };
    if mode != "check" && mode != "add" {
        return Err("Unknown favorites action.".into());
    }
    let address: SocketAddrV4 = address.parse().map_err(|_| "Invalid server address.")?;
    let game: u16 = game_port.parse().map_err(|_| "Invalid game port.")?;
    if game == 0 || address.port() == 0 {
        return Err("Ports must be between 1 and 65535.".into());
    }
    env::set_var("SteamAppId", APP.to_string());
    env::set_var("SteamGameId", APP.to_string());
    let mut error = [0; 1024];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs() as u32;
    // This short-lived process owns the Steam API session for the selected account.
    unsafe {
        if steam::SteamAPI_InitFlat(&mut error)
            != steam::ESteamAPIInitResult::k_ESteamAPIInitResult_OK
        {
            return Err(
                "Open Steam and sign in with an account that owns The Isle, then try again.".into(),
            );
        }
        let mm = steam::SteamAPI_SteamMatchmaking_v009();
        let ip = u32::from(*address.ip());
        let query = address.port();
        let result: Result<&str, String> = if mm.is_null() {
            Err("Steam matchmaking is unavailable.".into())
        } else if is_saved(mm, ip, game, query) {
            Ok("saved")
        } else if mode == "check" {
            Ok("missing")
        } else {
            steam::SteamAPI_ISteamMatchmaking_AddFavoriteGame(
                mm, APP, ip, game, query, FAVORITE, now,
            );
            if is_saved(mm, ip, game, query) {
                Ok("saved")
            } else {
                Err("Steam did not confirm the favorite. Check Steam → View → Game Servers → Favorites.".into())
            }
        };
        steam::SteamAPI_Shutdown();
        println!("{}", result?);
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
