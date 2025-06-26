pub mod middleware;
pub mod structs;

pub const USER_AGENT: &str = concat!(
    "DiscordBot (https://github.com/deadshot465/travel-agency, ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

pub const GPT_41: &str = "gpt-4.1";
pub const GEMINI_25_PRO: &str = "google/gemini-2.5-pro";
pub const GEMINI_25_FLASH: &str = "google/gemini-2.5-flash";

pub const DISCORD_ROOT_ENDPOINT: &str = "https://discord.com/api/v10";
pub const DISCORD_INTERACTION_CALLBACK_ENDPOINT: &str =
    "/interactions/$INTERACTION_ID/$INTERACTION_TOKEN/callback";
pub const DISCORD_INTERACTION_EDIT_ENDPOINT: &str =
    "/webhooks/$APPLICATION_ID/$INTERACTION_TOKEN/messages/@original";
pub const DISCORD_CREATE_THREAD_ENDPOINT: &str =
    "/channels/$CHANNEL_ID/messages/$MESSAGE_ID/threads";
