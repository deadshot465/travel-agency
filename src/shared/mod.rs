#![allow(dead_code)]
use serenity::all::Colour;

pub mod middleware;
pub mod structs;
pub mod utility;

pub const USER_AGENT: &str = concat!(
    "DiscordBot (https://github.com/deadshot465/travel-agency, ",
    env!("CARGO_PKG_VERSION"),
    ")"
);

pub const TEMPERATURE_LOW: f32 = 0.3;
pub const TEMPERATURE_MEDIUM: f32 = 0.7;
pub const TEMPERATURE_HIGH: f32 = 1.0;

pub const PLAN_COLLECTION_NAME: &str = "travel_agency_plans";
pub const PLAN_MAPPING_COLLECTION_NAME: &str = "travel_agency_plan_mappings";

pub const GPT_41: &str = "gpt-4.1";
pub const GEMINI_25_PRO: &str = "google/gemini-2.5-pro";
pub const GEMINI_25_FLASH: &str = "google/gemini-2.5-flash";
pub const CHAT_GPT_4O_LATEST: &str = "chatgpt-4o-latest";
pub const O3: &str = "o3";
pub const O3_PRO: &str = "o3-pro";
pub const SONNET_4: &str = "anthropic/claude-sonnet-4";
pub const OPUS_4: &str = "anthropic/claude-opus-4";
pub const GROK_3: &str = "x-ai/grok-3";
pub const GROK_4: &str = "x-ai/grok-4";
pub const DEEP_SEEK_V3: &str = "deepseek-chat";
pub const DEEP_SEEK_R1: &str = "deepseek-reasoner";
pub const GLM_4_PLUS: &str = "GLM-4-Plus";
pub const STEP_2_16K: &str = "step-2-16k";
pub const QWEN_MAX: &str = "qwen/qwen-max";
pub const QWEN_3_235B_A22B: &str = "qwen/qwen3-235b-a22b";
pub const DOUBAO_SEED_16: &str = "doubao-seed-1-6-250615";
pub const KIMI_LATEST: &str = "kimi-latest";
pub const MISTRAL_LARGE: &str = "mistralai/mistral-large-2411";
pub const MINIMAX_M1: &str = "minimax/minimax-m1";
pub const ERNIE_45_300B_A47B: &str = "baidu/ernie-4.5-300b-a47b";

pub const DISCORD_ROOT_ENDPOINT: &str = "https://discord.com/api/v10";
pub const DISCORD_INTERACTION_CALLBACK_ENDPOINT: &str =
    "/interactions/$INTERACTION_ID/$INTERACTION_TOKEN/callback";
pub const DISCORD_INTERACTION_EDIT_ENDPOINT: &str =
    "/webhooks/$APPLICATION_ID/$INTERACTION_TOKEN/messages/@original";
pub const DISCORD_CREATE_THREAD_ENDPOINT: &str =
    "/channels/$CHANNEL_ID/messages/$MESSAGE_ID/threads";

pub const EMBED_COLOR: Colour = Colour::from_rgb(147, 156, 149);
