use dotenvy::dotenv;
use std::collections::HashMap;
use std::error::Error;
use std::{env, io};
use std::{thread, time::Duration};
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::fdo::PropertiesProxy;
use zvariant::Array;
use zvariant::Value;

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

struct MediaConn {
    conn: Connection,
    whitelist: Vec<String>,
}
impl MediaConn {
    async fn new(whitelist: Vec<String>) -> Result<MediaConn, zbus::Error> {
        let conn = Connection::session().await?;
        Ok(Self { conn, whitelist })
    }

    async fn analyze(
        &self,
        service_name: String,
    ) -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {
        let mut output: HashMap<String, Vec<String>> = HashMap::new();
        let props = PropertiesProxy::builder(&self.conn)
            .destination(service_name)?
            .path("/org/mpris/MediaPlayer2")?
            .build()
            .await?;

        let playback_status: String = props
            .get(
                zbus::names::InterfaceName::from_static_str("org.mpris.MediaPlayer2.Player")
                    .unwrap(),
                "PlaybackStatus",
            )
            .await?
            .downcast_ref::<String>()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "Unknown".to_string());
        output.insert("Status".into(), vec![playback_status]);

        let playback_pos = props
            .get(
                zbus::names::InterfaceName::from_static_str("org.mpris.MediaPlayer2.Player")
                    .unwrap(),
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
                zbus::names::InterfaceName::from_static_str("org.mpris.MediaPlayer2.Player")
                    .unwrap(),
                "Metadata",
            )
            .await?;
        let metadata_value: Value = metadata_raw.downcast_ref::<Value>()?.clone();

        if let Value::Dict(dict_iter) = metadata_value {
            // println!("trying..");
            let mut meta_map = HashMap::new();
            for (k, v) in dict_iter.iter() {
                if let Value::Str(name) = k {
                    meta_map.insert(name.to_string(), v);
                }
            }
            // println!("{:?}", meta_map);

            if let Some(v) = meta_map.get("xesam:title") {
                // println!("{}", v);

                if let Ok(title) = v.downcast_ref::<String>() {
                    println!("Title: {}", title);
                    output.insert("Title".into(), vec![title]);
                }
            }

            if let Some(v) = meta_map.get("xesam:url") {
                // println!("{}", v);

                if let Ok(url) = v.downcast_ref::<String>() {
                    println!("url: {}", url);
                    let mut found = false;
                    for whitelisted in &self.whitelist {
                        println!("\n\n--- {}", whitelisted);
                        if url.contains(whitelisted) {
                            output.insert("url".into(), vec![url]);
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(io::Error::new(io::ErrorKind::Other, "Not whitelisted").into());
                    }
                }
            } else {
                return Err(io::Error::new(io::ErrorKind::Other, "Not found url").into());
            }

            if let Some(v) = meta_map.get("xesam:album") {
                // println!("{}", v);

                if let Ok(album) = v.downcast_ref::<String>() {
                    println!("Album: {}", album);
                    output.insert("Album".into(), vec![album]);
                }
            }

            if let Some(v) = meta_map.get("xesam:artist") {
                if let Ok(artist_arr) = v.downcast_ref::<Array>() {
                    // println!("{}", v);
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

    async fn get_media_info(&self) -> Result<HashMap<String, Vec<String>>, Box<dyn Error>> {

        let dbus_proxy = DBusProxy::new(&self.conn).await?;

        let names = dbus_proxy.list_names().await?;

        let mut media_services: Vec<String> = vec![];


        for name in names {
            // println!("{}", name);
            if name.starts_with("org.mpris.MediaPlayer2.") {
                println!("Found player: {}", name);
                media_services.push(name.to_string());
            }
        }
        // println!("_---\n {:?}", media_services);

        let mut final_output: Result<HashMap<String, Vec<String>>, Box<dyn Error>> =
            Err(io::Error::new(io::ErrorKind::Other, "No media").into());
        for service in media_services {
            let out = self.analyze(service).await;
            match out {
                Ok(out_ex) => {
                    if out_ex.get("Status").unwrap()[0] == "Playing" {
                        return Ok(out_ex);
                    } else {
                        final_output = Ok(out_ex);
                    }
                }
                Err(_) => continue,
            }
        }
        final_output
    }
}

use tokio::runtime::Runtime;

fn main() -> Result<(), ()> {
    let rt = Runtime::new().unwrap();
    dotenv().ok();

    let mut drpc = discord_rpc_client::Client::new(
        env::var("APP_ID")
            .expect("Set an APP ID")
            .parse::<u64>()
            .unwrap(),
    );
    drpc.start();
    let mediaconn = rt.block_on(MediaConn::new(vec!["music".into()])).unwrap();

    let mut pos: String = "0".to_string();
    loop {
        thread::sleep(Duration::from_millis(2000));
        let out: HashMap<String, Vec<String>>;
        match rt.block_on(mediaconn.get_media_info()) {
            Ok(o) => out = o,
            Err(e) => {
                println!("{}", e);

                drpc.clear_activity().unwrap();
                continue;
            }
        };

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
    }
}
