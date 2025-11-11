use dotenvy::dotenv;
use serde::Deserialize;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
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

#[derive(Deserialize, Debug)]
struct Config {
    pub keyword_whitelist: Vec<String>,
    pub use_whitelist: bool,
    pub play_no_url: bool,
    pub artist_keyword_blacklist: Vec<String>,
    pub use_artist_blacklist: bool,
    pub embolden_titles: bool,
}

struct MediaConn {
    conn: Connection,
    config: Config,
}
impl MediaConn {
    async fn new() -> Result<MediaConn, zbus::Error> {
        let conn = Connection::session().await?;
        let file = fs::read_to_string("config.json")?;
        let config: Config = serde_json::from_str(&file).unwrap();

        // let use_whitelist = whitelist_inst.use_whitelist;
        // let play_no_url = whitelist_inst.play_no_url;
        // let whitelist = whitelist_inst.keyword_whitelist;

        Ok(Self { conn, config })
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

                    if !self.config.use_whitelist {
                        output.insert("url".into(), vec![url]);
                    } else {
                        let mut found = false;
                        for whitelisted in &self.config.keyword_whitelist {
                            println!("\n\n--- {}", whitelisted);
                            if url.contains(whitelisted) {
                                output.insert("url".into(), vec![url]);
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            return Err(
                                io::Error::new(io::ErrorKind::Other, "Not whitelisted").into()
                            );
                        }
                    }
                }
            } else if !self.config.play_no_url {
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
                    if self.config.use_artist_blacklist {
                        for artist in art_vec {
                            for keyword in &self.config.artist_keyword_blacklist {
                                if artist.contains(keyword) {
                                    return Err(io::Error::new(
                                        io::ErrorKind::Other,
                                        "Artist blacklisted",
                                    )
                                    .into());
                                }
                            }
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
                Err(e) => {
                    println!("Output not allowed: {}", e);
                    continue;
                }
            }
        }
        final_output
    }
}

use tokio::runtime::Runtime;

use math_text_transform::{math_bold, math_italic};

fn change(s: &String, modifier: &dyn Fn(char) -> Option<char>) -> Result<String, io::Error> {
    let mut out: String = "".into();

    let ignore: Vec<char> = vec!['(', ')', ' ', '[', ']', '"', ',', '\n', '\u{a0}', '\'', '-'];
    let stop_chars: Vec<char> = vec!['(', '['];
    for i in s.chars() {
        if let Some(variant) = modifier(i) {
            out.push(variant);
        } else if stop_chars.contains(&i) {
            break;
            // out.push(i);
        } else if ignore.contains(&i) {
            out.push(i);
        } else {
            // println!("EEEEEEEEEEE");
            println!("Wrong char code: {} ----{}", i as u32, i);

            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Unable to fully transform",
            ));
        }
    }

    out.push_str(s.chars().skip(out.chars().count()).collect::<String>().as_str());

    Ok(out)
}


fn truncate_utf8_bytes(s: String, max_bytes: usize) -> String {
    let mut len = 0;
    let mut out = String::new();

    for c in s.chars() {
        let char_len = c.len_utf8();
        if len + char_len > max_bytes {
            out.push_str("...");
            break;
        }
        len += char_len;
        out.push(c);
    }

    out
}


use math_text_transform::math_bold_script;
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
    let mediaconn = rt.block_on(MediaConn::new()).unwrap();

    let mut pos: String = "0".to_string();
    loop {
        thread::sleep(Duration::from_millis(2000));
        println!("-----------NEW HEARTBEAT-------");
        let out: HashMap<String, Vec<String>>;
        match rt.block_on(mediaconn.get_media_info()) {
            Ok(o) => out = o,
            Err(e) => {
                println!("ERROR: {}", e);

                drpc.clear_activity().unwrap();
                continue;
            }
        };

        let mut title = out.get("Title").unwrap()[0].clone();

        let artists = out.get("Artists").unwrap();
        let mut artstr: String = artists.join(", ");

        if mediaconn.config.embolden_titles {
            if let Ok(outx) = change(&title, &math_bold) {
                title = outx;
            }
            if let Ok(outx) = change(&artstr, &math_italic) {
                artstr = outx;
            }
            // title = title.to_math_bold();
        }
        // println!("TITLE : {}", title);

        let album = out.get("Album").unwrap()[0].clone();

        let playing = out.get("Status").unwrap()[0].clone();

        let state;
        match playing {
            p if p == "Playing" => {
                state = format!("{}: {}",change(&"By".to_string(), &math_bold_script).unwrap() , artstr);
                pos = out.get("Position").unwrap()[0].clone();
                println!("POS::: {}", pos.clone())
            }
            _ => state = format!("Paused @ {} • \nBy: {} ", pos, artstr),
        };
        println!("{}", state);

        println!("\n---Setting activity---");
        drpc.set_activity(|act| {
            act.state(state)
                .details(truncate_utf8_bytes(format!("🎵 {} • 💿 {}", title, album), 125))
                .assets(|ass| ass.large_image("arch_icon").large_text("#ARCHONTOP"))
        })
        .expect("Failed to set activity");
        println!("\n---SUCCESS---");
    }
}
