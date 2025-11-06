use std::{thread, time::Duration};
use std::collections::HashMap;
use zbus::Connection;
use zbus::fdo::PropertiesProxy;
use zvariant::Array;
use zvariant::Value;
use zbus::fdo::DBusProxy;
use dotenvy::dotenv;
use std::env;



async fn format_time(mut secs: i64) -> String {
    let mut mins: i64 = 0;
    while secs > 60 {
        secs -= 60;
        mins += 1;
    }
    let mut secs = secs.to_string();
    if secs.len() < 2 {
        secs.insert(0, '0');
    }
    return format!("{}:{}", mins, secs);
}

async fn test() -> zbus::Result<HashMap<String, Vec<String>>> {
    let mut output: HashMap<String, Vec<String>> = HashMap::new();

    let conn = Connection::session().await?;

    let dbus_proxy = DBusProxy::new(&conn).await?;

    let names = dbus_proxy.list_names().await?;

    let mut service_name = "".to_string();

    for name in names {
        // println!("{}", name);
        if name.starts_with("org.mpris.MediaPlayer2.") {
            println!("Found player: {}", name);
            service_name = name.to_string();
        }
    }



    let props = PropertiesProxy::builder(&conn)
        .destination(service_name)?
        .path("/org/mpris/MediaPlayer2")?
        .build()
        .await?;

    let playback_status: String = props
        .get(
            zbus::names::InterfaceName::from_static_str("org.mpris.MediaPlayer2.Player").unwrap(),
            "PlaybackStatus",
        )
        .await?
        .downcast_ref::<String>()
        .map(|s| s.clone())
        .unwrap_or_else(|_| "Unknown".to_string());
    output.insert("Status".into(), vec![playback_status]);

    let playback_pos = props
        .get(
            zbus::names::InterfaceName::from_static_str("org.mpris.MediaPlayer2.Player").unwrap(),
            "Position",
        )
        .await?
        .downcast_ref::<i64>()
        .unwrap_or_else(|_| 0)
        / 1000000;
    println!("POSITION : {:?}", playback_pos);
    output.insert("Position".into(), vec![format_time(playback_pos).await]);

    let metadata_raw = props
        .get(
            zbus::names::InterfaceName::from_static_str("org.mpris.MediaPlayer2.Player").unwrap(),
            "Metadata",
        )
        .await?;
    let metadata_value: Value = metadata_raw.downcast_ref::<Value>()?.clone();

    if let Value::Dict(dict_iter) = metadata_value {
        println!("trying..");
        let mut meta_map = HashMap::new();
        for (k, v) in dict_iter.iter() {
            if let Value::Str(name) = k {
                meta_map.insert(name.to_string(), v);
            }
        }
        println!("{:?}", meta_map);

        if let Some(v) = meta_map.get("xesam:title") {
            println!("{}", v);

            if let Ok(title) = v.downcast_ref::<String>() {
                println!("Title: {}", title);
                output.insert("Title".into(), vec![title]);
            }
        }

        if let Some(v) = meta_map.get("xesam:album") {
            println!("{}", v);

            if let Ok(album) = v.downcast_ref::<String>() {
                println!("Album: {}", album);
                output.insert("Album".into(), vec![album]);
            }
        }

        if let Some(v) = meta_map.get("xesam:artist") {
            if let Ok(artist_arr) = v.downcast_ref::<Array>() {
                println!("{}", v);
                output.insert("Artists".into(), vec![]);
                let art_vec = output.get_mut("Artists").unwrap();
                for artist_v in artist_arr.iter() {
                    if let Ok(artist) = artist_v.downcast_ref::<String>() {
                        println!("Artist: {} ", artist);
                        art_vec.push(artist);
                        
                    }
                }
            }
        }


        if let Some(v) = meta_map.get("mpris:artUrl") {
            if let Value::Str(url) = v {
                println!("Art url: {}", url);
            }
        }
    } else {
        println!("Metadata not a dict: {:?}", metadata_value);
    }

    Ok(output)
}

use tokio::runtime::Runtime;

fn main() -> Result<(), ()> {
    let rt = Runtime::new().unwrap();
    dotenv().ok();



    
    let mut drpc = discord_rpc_client::Client::new(env::var("APP_ID").expect("Set an APP ID").parse::<u64>().unwrap());
    drpc.start();

    let mut pos: String = "0".to_string();
    loop {
        let out = rt.block_on(test()).unwrap();
        let title = &out.get("Title").unwrap()[0];
        let artists = out.get("Artists").unwrap();
        let artstr: String = artists.join(", ");

        let album = out.get("Album").unwrap()[0].clone();

        let playing = out.get("Status").unwrap()[0].clone();

        let state;
        match playing {
            p if p == "Playing" => {
                state = format!("By: {}", artstr);
                pos = out.get("Position").unwrap()[0].clone();
                println!("POS::: {}", pos.clone())
            }
            _ => state = format!("Paused at {} • \nBy: {} ", pos, artstr),
        };
        println!("{}", state);

        drpc.set_activity(|act| {
            act.state(state)
                .details(format!("🎵 {} • 💿 {}", title, album))
                .assets(|ass| ass.large_image("arch_icon").large_text("#ARCHONTOP"))
        })
        .expect("Failed to set activity");

        thread::sleep(Duration::from_millis(2000));
    }
}
