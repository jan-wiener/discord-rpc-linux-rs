# discord-rpc-linux-rs
Discord rich presence for media written in rust. 

**Only compatible with mpris/MediaPlayer2, which works on KDE Plasma**

Very bad code btw

Create a discord app with the name you want people to see on your profile
(something like "Listening to" or "Watching")
Create a .env 
```.env
APP_ID = your_discord_app_id
```

Add whitelisted keywords to whitelist.json
non-whitelist mode not implemented

code checks xesam:url for the keywords in whitelist
(i.e. "music" allows music.youtube.com to update the rich presence, but some websites/apps dont show xesam:url and those wont work, might fix later) 



compile with:
```bash
cargo build --release
```