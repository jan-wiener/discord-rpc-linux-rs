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

compile with:
```bash
cargo build --release
```