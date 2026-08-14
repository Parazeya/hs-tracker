// diagnostic: put one made-up run in Discord, so the application id, the
// uploaded artwork and the shape of the two lines can be checked without
// farming for an hour first
//
//     cargo run --example presence
//
// Discord has to be running. The status stays up until Enter is pressed, since
// it belongs to this process and dies with it.

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

const APP_ID: &str = "1537867623281467452";

fn main() {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64
        - 107 * 60 * 1000; // an hour and three quarters into the run

    let mut client = DiscordIpcClient::new(APP_ID);
    if let Err(e) = client.connect() {
        eprintln!("no Discord to talk to: {e}");
        eprintln!("is the client running, and is this the machine it runs on?");
        return;
    }

    let activity = Activity::new()
        .details("Act 8 · Zone 2 · Hell HC")
        .state("4 SS · 1 Unholy · 7.3k gold")
        .assets(
            Assets::new()
                .large_image("logo")
                .large_text("HS Tracker · level 100 · hero level 137")
                .small_image("satanic")
                .small_text("Standing in the Satanic Zone"),
        )
        .timestamps(Timestamps::new().start(started));

    match client.set_activity(activity) {
        Ok(()) => println!("status sent — look at your own profile in Discord"),
        Err(e) => {
            eprintln!("Discord refused the activity: {e}");
            return;
        }
    }
    // the reply carries whatever Discord thought of it, and reading it is also
    // what keeps the pipe from filling up
    match client.recv() {
        Ok((_, reply)) => println!("Discord answered: {reply}"),
        Err(e) => eprintln!("no answer: {e}"),
    }

    println!("press Enter to take it down");
    let _ = std::io::stdin().read(&mut [0u8]);
    let _ = client.clear_activity();
    let _ = client.close();
}
