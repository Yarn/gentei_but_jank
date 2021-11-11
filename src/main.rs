// use ::serenity::async_trait;
// use ::serenity::client::{Client, Context, EventHandler};
// use ::serenity::model::channel::Message;
// use ::serenity::framework::standard::{
//     StandardFramework,
//     CommandResult,
//     macros::{
//         command,
//         group
//     }
// };

use std::fmt;
use std::{ time::Duration };
use anyhow::Context as _;

use poise::serenity::FutureExt;
use poise::serenity_prelude as serenity;
use poise::serenity::model::id::UserId;

mod util;
use util::{ to_i, from_i };

mod check_wrapper;
mod verification;
mod roles_sync;
mod url_parse;
mod youtube_req;

lazy_static::lazy_static! {
    static ref GOOJF: String = {
        std::env::var("goojf").expect("goojf environment variable not set")
    };
}

const GUIDE: &str = include_str!("guide_text.md");

// #[group]
// #[commands(ping)]
// struct General;

// struct Handler;

// #[async_trait]
// impl EventHandler for Handler {}

// #[tokio::main]
// async fn main() {
//     let framework = StandardFramework::new()
//         .configure(|c| c.prefix("~")) // set the bot's prefix to "~"
//         .group(&GENERAL_GROUP);

//     // Login with a bot token from the environment
//     let token = env::var("DISCORD_TOKEN").expect("token");
//     let mut client = Client::builder(token)
//         .event_handler(Handler)
//         .framework(framework)
//         .await
//         .expect("Error creating client");

//     // start listening for events by starting a single shard
//     if let Err(why) = client.start().await {
//         println!("An error occurred while running the client: {:?}", why);
//     }
// }

// #[command]
// async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
//     msg.reply(ctx, "Pong!").await?;

//     Ok(())
// }

#[derive(Debug)]
struct HumanError(String);

impl fmt::Display for HumanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

use sqlx::{ PgPool };

// use crate::verification::HumanContext;

#[derive(Debug)]
enum GetPoolError {
    MissingEnv,
    Sqlx(sqlx::Error),
}

async fn get_pool() -> Result<PgPool, GetPoolError> {
    let pg_url = match std::env::var("pg_url") {
        Ok(token) => token,
        Err(_e) => {
            // eprintln!("Failed to get env var pg_url: {:?}", e);
            return Err(GetPoolError::MissingEnv);
        }
    };
    
    use sqlx::postgres::PgPoolOptions;
    
    let pool: PgPool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&pg_url).await.map_err(|e| GetPoolError::Sqlx(e))?;
    
    Ok(pool)
}

#[derive(Debug)]
pub struct Config {
    token_video: String,
    token_channel: String,
    goojf: String,
}

// type Data = ();
pub struct Data {
    pool: PgPool,
    config: Config,
    guide_text: Vec<String>,
}
// type Error = Box<dyn std::error::Error + Send + Sync>;
type Error = anyhow::Error;
type Context<'a> = poise::Context<'a, Data, Error>;

/// Register application commands in this guild or globally
///
/// Run with no arguments to register in guild, run with argument "global" to register globally.
#[poise::command(prefix_command, hide_in_help)]
async fn register(ctx: Context<'_>, #[flag] global: bool) -> Result<(), Error> {
    poise::samples::register_application_commands(ctx, global).await?;

    Ok(())
}

/// Display your or another user's account creation date
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn age(
    ctx: Context<'_>,
    #[description = "Selected user"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let user = user.as_ref().unwrap_or(ctx.author());
    ctx.say(format!("{}'s account was created at {}", user.name, user.created_at())).await?;
    
    Ok(())
}

/// Request a DM from the bot
#[poise::command(prefix_command, slash_command)]
pub async fn dmme(
    ctx: Context<'_>,
) -> Result<(), Error> {
    // ctx.author().direct_message(&ctx.discord(), |m| {
    //     m
    //         // .content("8")
    //         .embed(|e| {
    //             e.field("Guide", GUIDE.trim(), true)
    //         })
    // }).await?;
    
    // let parts = GUIDE.split(">---");
    let parts = ctx.data().guide_text.iter().map(|x| x.as_str());
    
    // let part = parts.next().ok_or_else(|| anyhow::anyhow!("guide has no parts"))?;
    for part in parts {
        ctx.author().direct_message(&ctx.discord(), |m| {
            m
                // .content("8")
                .embed(|e| {
                    e.field("Guide", part.trim(), true)
                })
                
        }).await?;
    }
    
    Ok(())
}

/// DM a setup guide to the user
#[poise::command(prefix_command, slash_command)]
pub async fn guide(
    ctx: Context<'_>,
) -> Result<(), Error> {
    // let parts = GUIDE.split(">---");
    let parts = ctx.data().guide_text.iter().map(|x| x.as_str());
    
    for part in parts {
        ctx.send(|m| {
            m
                .embed(|e| {
                    e.field("Guide", part.trim(), true)
                })
                // .attachment((GUIDE.as_bytes(), "Guide.md").into())
        }).await?;
    }
    // ctx.send(|m| {
    //     m
    //         .embed(|e| {
    //             e.field("Guide", GUIDE[..1000].trim(), true)
    //         })
    //         .embed(|e| {
    //             e.field("a", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true)
    //         })
    // }).await?;
    
    Ok(())
}

fn parse_channel_str(yt_channel_id: &str) -> Result<(String, i64), Error> {
    let chan_id = if url_parse::is_url(&yt_channel_id) {
        let yt_video_url = yt_channel_id
            .strip_prefix("<").unwrap_or(&yt_channel_id)
            .strip_suffix(">").unwrap_or(&yt_channel_id);
        
        let yt_channel_id = match url_parse::extract_channel_id(yt_video_url) {
            Some(id) => id,
            None => {
                let err = anyhow::anyhow!("could not extract channel id from url {}", yt_channel_id)
                    .context(HumanError("could not extract channel id from url".into()));
                return Err(err)
            }
        };
        yt_channel_id
    } else {
        yt_channel_id.to_string()
    };
    
    if let Some((chan_id, n_str)) = chan_id.split_once("'") {
        let n: i64 = if n_str.is_empty() {
            1
        } else {
            n_str.parse().context(HumanError("invalid channel n value".into()))?
        };
        if !(0 <= n && n < 100) {
            let err = anyhow::anyhow!("invalid channel n value {}", n)
                .context(HumanError("invalid channel n value".into()));
            return Err(err)
        }
        Ok((chan_id.into(), n))
    } else {
        Ok((chan_id, 0))
    }
}

/// Set up a new verification.
#[poise::command(prefix_command, slash_command)]
pub async fn new_token(
    ctx: Context<'_>,
    #[description = "Youtube Channel Id"] yt_channel_id: Option<String>,
) -> Result<(), Error> {
    let yt_channel_id = yt_channel_id.unwrap_or_else(|| ctx.data().config.token_channel.clone());
    
    let mut transaction = ctx.data().pool.begin().await?;
    
    // if url_parse::is_url(&yt_channel_id) {
    //     let yt_video_url = yt_channel_id
    //         .strip_prefix("<").unwrap_or(&yt_channel_id)
    //         .strip_suffix(">").unwrap_or(&yt_channel_id);
        
    //     yt_channel_id = match url_parse::extract_channel_id(yt_video_url) {
    //         Some(id) => id,
    //         None => {
    //             let err = anyhow::anyhow!("could not extract channel id from url {}", yt_channel_id)
    //                 .context(HumanError("could not extract channel id from url".into()));
    //             return Err(err)
    //         }
    //     };
    // }
    let (yt_channel_id, yt_channel_n) = parse_channel_str(&yt_channel_id)?;
    
    let user_id: u64 = ctx.author().id.0;
    let token = util::gen_token();
    
    sqlx::query!(r#"
        INSERT INTO genteib.users
                ("discord_id", "yt_channel_id", "yt_channel_n", "token")
        VALUES  ($1,           $2,              $3,             $4     )
        ON CONFLICT ("discord_id", "yt_channel_id", "yt_channel_n")
            DO UPDATE SET
                token = $3,
                last_verified = NULL,
                last_channel_verified = NULL,
                last_checked = NULL,
                failed_checks = NULL,
                yt_video_id = NULL,
                yt_comment_id = NULL,
                user_yt_channel_id = NULL,
                extra = '{}'
    "#,
        to_i(user_id),
        &yt_channel_id,
        yt_channel_n,
        &token,
    )
        // .bind(to_i(user_id))
        // .bind(&yt_channel_id)
        // .bind(&token)
        // .bind(tag_name)
        // .bind(tag_time)
        // .bind(msg.guild_id.as_ref().map(|x| to_i(x.0)))
        // .bind(to_i(msg.author.id.0))
        // .bind(to_i(msg.id.0))
        .execute(&mut transaction).await?;
    
    transaction.commit().await?;
    
    ctx.say(format!("{}", token)).await?;
    
    Ok(())
}

/// Remove configuration for a channel
#[poise::command(prefix_command, slash_command)]
pub async fn clear_token(
    ctx: Context<'_>,
    #[description = "Youtube Channel"] yt_channel_id: String,
) -> Result<(), Error> {
    let (yt_channel_id, yt_channel_n) = parse_channel_str(&yt_channel_id)?;
    let ref pool = ctx.data().pool;
    let user_id: u64 = ctx.author().id.0;
    
    sqlx::query!(r#"
        delete from genteib.users
        where
            yt_channel_id = $2 AND
            yt_channel_n = $3 AND
            discord_id = $1
    "#,
        to_i(user_id),
        &yt_channel_id,
        yt_channel_n,
    )
        // .bind(to_i(user_id))
        // .bind(&yt_channel_id)
        .execute(pool).await?;
    
    poise::say_reply(
        ctx,
        "thank you thank you",
    ).await?;
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
pub async fn force_token(
    ctx: Context<'_>,
    discord_id: u64,
    yt_channel_id: String,
    token: String,
) -> Result<(), Error> {
    let ref pool = ctx.data().pool;
    let (yt_channel_id, yt_channel_n) = parse_channel_str(&yt_channel_id)?;
    
    sqlx::query(r#"
        UPDATE genteib.users
            SET
                token = $4
            WHERE
                discord_id = $1 AND
                yt_channel_id = $2 AND
                yt_channel_n = $3
    "#)
        .bind(to_i(discord_id))
        .bind(yt_channel_id)
        .bind(yt_channel_n)
        .bind(token)
        .execute(pool).await?;
    
    poise::say_reply(
        ctx,
        "thank you thank you",
    ).await?;
    
    Ok(())
}

pub async fn set_comment_inner(
    ctx: Context<'_>,
    yt_channel_id: &str,
    yt_channel_n: i64,
    yt_video_id: &str,
    yt_comment_id: &str,
) -> Result<(), Error> {
    // let (ref yt_channel_id, yt_channel_n) = parse_channel_str(yt_channel_id)?;
    
    let mut transaction = ctx.data().pool.begin().await?;
    
    let user_id: u64 = ctx.author().id.0;
    let token = util::gen_token();
    
    sqlx::query(r#"
        INSERT INTO genteib.users
                ("discord_id", "yt_channel_id", "token")
        VALUES  ($1,           $2,              $3     )
        ON CONFLICT ("discord_id", "yt_channel_id", "yt_channel_n")
            DO NOTHING
    "#)
        .bind(to_i(user_id))
        .bind(yt_channel_id)
        .bind(&token)
        .execute(&mut transaction).await?;
    
    sqlx::query(r#"
        UPDATE genteib.users
            SET
                yt_video_id = $3,
                yt_comment_id = $4
            WHERE
                discord_id = $1 AND
                yt_channel_id = $2
    "#)
        .bind(to_i(user_id))
        .bind(yt_channel_id)
        .bind(yt_video_id)
        .bind(yt_comment_id)
        .execute(&mut transaction).await?;
    
    transaction.commit().await?;
    
    let res = verification::update_verification(&ctx.data().pool, user_id, yt_channel_id, yt_channel_n).await?;
    
    match res.update_roles(&ctx.data().pool, &ctx.discord().http).await {
        Ok(None) => (),
        Ok(Some(res)) => {
            for err in res.role_errors {
                println!("err updating role {:?}", err);
            }
        },
        Err(err) => {
            println!("err updating roles {:?}", err);
        }
    }
    
    if !res.is_member {
        let mut msg = "thank you thank you (not a member)".to_string();
        use std::fmt::Write;
        for err in res.errors {
            write!(msg, "\n`  `{}", err).unwrap();
        }
        ctx.say(msg).await?;
    } else {
        // ctx.reply("thank you thank you").await?;
        poise::say_reply(
            ctx,
            "thank you thank you",
        ).await?;
    }
    Ok(())
}

/// Sets comment for verification
#[poise::command(prefix_command, slash_command)]
pub async fn set_comment(
    ctx: Context<'_>,
    #[description = "Video Url"] yt_video_url: String,
) -> Result<(), Error> {
    let yt_video_url = yt_video_url
        .strip_prefix("<").unwrap_or(&yt_video_url)
        .strip_suffix(">").unwrap_or(&yt_video_url);
    
    let (video_id, comment_id) = match url_parse::extract_video_comment_id(&yt_video_url) {
        Some((video_id, Some(comment_id))) => (video_id, comment_id),
        _ => {
            let err = anyhow::anyhow!("could not extract video or comment id from url {}", yt_video_url)
                .context(HumanError("Could not extract video and comment id from url".into()));
            return Err(err);
        }
    };
    
    let req_url = format!("https://www.youtube.com/watch?v={}", video_id);
    let channel_id = youtube_req::get_channel_id(&req_url).await
        .map_err(|e| {
            e.context(HumanError("Could not fetch channel id for video".into()))
        })?;
    
    set_comment_inner(ctx, &channel_id, 0, &video_id, &comment_id).await
}

/// Sets comment for verification
#[poise::command(prefix_command, slash_command)]
pub async fn set_comment_b(
    ctx: Context<'_>,
    #[description = "Youtube Channel Id"] yt_channel_id: String,
    #[description = "Youtube Video Id"]   yt_video_id: String,
    #[description = "Youtube Comment Id"] yt_comment_id: String,
) -> Result<(), Error> {
    let (yt_channel_id, yt_channel_n) = parse_channel_str(&yt_channel_id)?;
    
    set_comment_inner(ctx, &yt_channel_id, yt_channel_n, &yt_video_id, &yt_comment_id).await
}

async fn status_inner(
    ctx: Context<'_>,
    user_id: u64,
) -> Result<(), Error> {
    let ref pool = ctx.data().pool;
    
    let statuses = verification::get_statuses(pool, user_id).await
        .map_err(|e| { dbg!(&e); e })?;
    
    if statuses.is_empty() {
        poise::say_reply(ctx, "No configured channels").await?;
    }
    
    for status in statuses {
        poise::say_reply(ctx, &status.format_message()).await?;
    }
    
    Ok(())
}

/// Get verification status
#[poise::command(prefix_command, slash_command)]
pub async fn status(
    ctx: Context<'_>,
) -> Result<(), Error> {
    let user_id: u64 = ctx.author().id.0;
    
    status_inner(ctx, user_id).await
}

#[poise::command(prefix_command, owners_only)]
pub async fn statusu (
    ctx: Context<'_>,
    user_id: u64,
) -> Result<(), Error> {
    status_inner(ctx, user_id).await
}

#[poise::command(prefix_command, owners_only)]
pub async fn set_role(
    ctx: Context<'_>,
    server_id: u64,
    role_id: u64,
    channel_id: Option<String>,
) -> Result<(), Error> {
    let ref pool = ctx.data().pool;
    
    let role_id_str = role_id.to_string();
    
    use std::collections::BTreeMap;
    use sqlx::types::Json;
    if let Some(channel_id) = channel_id {
        let mut role_map = BTreeMap::new();
        role_map.insert(&role_id_str, &channel_id);
        
        sqlx::query(r#"
            INSERT INTO genteib.servers (server_id, roles)
            VALUES ($1, $4)
            --SET
            --    roles = 
            --WHERE
            --    
            ON CONFLICT ("server_id")
                DO UPDATE SET
                    roles = servers.roles || EXCLUDED.roles
        "#)
            .bind(to_i(server_id))
            .bind(to_i(role_id))
            .bind(&channel_id)
            .bind(Json(role_map))
            .execute(pool).await?;
    } else {
        sqlx::query(r#"
            UPDATE genteib.servers
            SET
                roles = roles - $2
            WHERE
                server_id = $1
        "#)
            .bind(to_i(server_id))
            .bind(role_id_str)
            .execute(pool).await?;
    }
    
    poise::say_reply(
        ctx,
        "thank you thank you",
    ).await?;
    
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
pub async fn test_check(
    ctx: Context<'_>,
    yt_video_id: String,
    yt_comment_id: String,
) -> Result<(), Error> {
    use check_wrapper::{check_member, Member, Not, NotFound};
    let res = check_member(&yt_video_id, &yt_comment_id).await?;
    
    match res {
        (_, Member{ channel_id, .. }) => {
            ctx.say(format!("member {}", channel_id)).await?;
        }
        (_, Not{ .. }) => {
            ctx.say("not member").await?;
        }
        (_, NotFound) => {
            ctx.say("could not find comment").await?;
        }
    }
    
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
pub async fn test_verify(
    ctx: Context<'_>,
    discord_id: u64,
    yt_channel_id: String,
) -> Result<(), Error> {
    use verification::update_verification;
    // let mut transaction = ctx.data().pool.begin().await?;
    let (yt_channel_id, yt_channel_n) = parse_channel_str(&yt_channel_id)?;
    
    update_verification(&ctx.data().pool, discord_id, &yt_channel_id, yt_channel_n).await
        .map_err(|e| { println!("{:?}", e); e })?;
    
    // transaction.commit().await?;
    poise::say_reply(
        ctx,
        "thank you thank you",
    ).await?;
    
    Ok(())
}

#[poise::command(prefix_command, owners_only)]
pub async fn sync_members(
    ctx: Context<'_>,
    guild_id: Option<u64>,
) -> Result<(), Error> {
    let guild_id = guild_id.or_else(|| ctx.guild_id().map(|g| g.0));
    let guild_id = match guild_id {
        Some(guild_id) => guild_id,
        None => {
            poise::say_reply(
                ctx,
                "must specify a guild id or run in a guild",
            ).await?;
            return Err(::anyhow::anyhow!("sync_members run without guild id").into());
        }
    };
    
    let ref http = ctx.discord().http;
    let ref pool = ctx.data().pool;
    
    // roles_sync::sync_roles(&ctx, guild_id).await
    roles_sync::sync_roles(&pool, &http, guild_id).await
        .map_err(|e| { dbg!(&e); e })?;
    
    Ok(())
}

/// Show this menu
#[poise::command(prefix_command, track_edits, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"] command: Option<String>,
) -> Result<(), Error> {
    poise::samples::help(
        ctx,
        command.as_deref(),
        "", // bottom text
        poise::samples::HelpResponseMode::Ephemeral,
    ).await?;
    Ok(())
}

async fn check_prefix<'a>(
    _ctx: &'a poise::serenity_prelude::Context,
    msg: &'a poise::serenity_prelude::Message,
    _data: &'a Data,
) -> Option<String> {
    if msg.guild_id.is_none() && !msg.content.starts_with(">>'") {
        Some("".into())
    } else {
        None
    }
}

async fn error_handler(error: Error, ctx: poise::ErrorContext<'_, Data, Error>) {
    let err_uuid = util::gen_uuid();
    if &err_uuid[17..] != "000000000000000" {
        println!("unexpected uuid tail {} on {}", &err_uuid[17..], err_uuid);
    }
    let err_uuid = &err_uuid[..17];
    
    // panic!("a");
    println!("ERROR ({}): {:?}", err_uuid, error);
    if let poise::ErrorContext::Command(ce_context) = ctx {
        let ctx = ce_context.ctx();
        
        use std::fmt::Write;
        let mut msg = String::new();
        write!(msg, "error ({})", err_uuid).unwrap();
        
        // let a: Option<&anyhow::Error> = error.downcast_ref::<anyhow::Error>();
        // println!("{:?}", a);
        let x: Option<&verification::HumanContext> = error.downcast_ref();
        // println!("{:?}", x);
        if let Some(e) = x {
            write!(msg, ": {}", e).unwrap();
        }
        
        if let Some(e) = error.downcast_ref::<HumanError>() {
            write!(msg, ": {}", e).unwrap();
        }
        
        if let Some(e) = error.downcast_ref::<poise::ArgumentParseError>() {
            write!(msg, ": {}", e).unwrap();
        }
        
        // trait Test: std::fmt::Display + Send + Sync + std::fmt::Debug + 'static + Sized {}
        
        // let x: Option<&Test> = error.downcast_ref();
        
        if let Err(err) = poise::say_reply(
            ctx,
            &msg,
            // "error",
        ).await {
            dbg!(err);
        }
    }
}

#[tokio::main]
async fn main() {
    let pool = get_pool().await.expect("failed to get pool");
    
    let token = std::env::var("discord_auth").expect("discord_auth env var not set");
    
    let owners: std::collections::HashSet<_> = std::env::var("owners")
        .expect("owners env var not set")
        .split(",")
        .map(|x| x.trim().parse().expect("invalid owner value"))
        .map(|x| UserId(x))
        .collect();
    
    let config = Config {
        token_channel: std::env::var("token_channel").expect("token_channel env_var not set"),
        token_video: std::env::var("token_video").expect("token_video env_var not set"),
        goojf: GOOJF.clone(),
        
    };
    
    let guide_text: Vec<String> = {
        let support_text = std::env::var("support_text").unwrap_or("".into());
        
        let parts = GUIDE
            .replace("{video_id}", &config.token_video)
            .replace("{channel_id}", &config.token_channel)
            .replace("{support_text}", &support_text)
            .split(">---")
            .map(|x| x.trim().to_string())
            .collect();
        parts
    };
    
    let cmd = std::env::args().skip(1).next();
    
    match cmd.as_ref().map(|s| s.as_str()) {
        Some("verify_daemon") => {
            println!("running verify daemon");
            // let client = poise::serenity::client::Client::builder(&token)
            //     .await.expect("serenity client start");
            use std::sync::Arc;
            use poise::serenity::CacheAndHttp;
            let http = Arc::new(poise::serenity::http::client::Http::new_with_token(&token));
            // let cache_http = poise::serenity::CacheAndHttp {
            //     // cache: Arc::new(serenity::cache::Cache::new()),
            //     // update_cache_timeout: None,
            //     http: http.clone(),
            //     ..CacheAndHttp::default(),
            // };
            let mut cache_http = CacheAndHttp::default();
            cache_http.http = http.clone();
            
            // return;
            // let http = client.cache_and_http.http.clone();
            loop {
                match verification::verify_pending(&pool, 100).await {
                    Ok(results) => {
                        for res in results {
                            match res.update_roles(&pool, &http).await {
                                Ok(None) => (),
                                Ok(Some(res)) => {
                                    for err in res.role_errors {
                                        println!("err updating role {:?}", err);
                                    }
                                },
                                Err(err) => {
                                    println!("err updating roles {:?}", err);
                                }
                            }
                            
                            async fn send_message(cache_http: &CacheAndHttp, user_id: u64, msg: &str) -> Result<(), anyhow::Error> {
                                let user_id = poise::serenity::model::id::UserId(user_id);
                                let user = user_id.to_user(cache_http).await?;
                                user.direct_message(cache_http, |m| {
                                    m.content(msg)
                                }).await?;
                                
                                Ok(())
                            }
                            
                            // println!("{:?}", res);
                            let msg = if res.became_member() {
                                Some(format!("Membership to {} ({}) is now verified", res.channel_name, res.yt_channel_id))
                            } else if res.became_non_member() {
                                let mut msg = format!("Membership to {} ({}) is no longer verified", res.channel_name, res.yt_channel_id);
                                use std::fmt::Write;
                                for err in res.errors {
                                    write!(msg, "\n`  `{}", err).unwrap();
                                }
                                Some(msg)
                            } else {
                                None
                            };
                            if let Some(msg) = msg {
                                match send_message(&cache_http, res.discord_id, &msg).await {
                                    Ok(()) => (),
                                    Err(err) => {
                                        println!("could not send become member message {:?}", err);
                                    }
                                }
                            }
                        }
                    },
                    Err(err) => { dbg!(err); },
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        },
        Some("sync_roles") => {
            println!("syncing roles");
            let rows: Vec<(i64,)> = sqlx::query_as(r#"
                SELECT server_id
                FROM genteib.servers
            "#)
                .fetch_all(&pool).await
                .expect("get server list");
            
            let http = poise::serenity::http::client::Http::new_with_token(&token);
            
            for (server_id,) in rows {
                let server_id = from_i(server_id);
                println!("syncing roles for server {}", server_id);
                match roles_sync::sync_roles(&pool, &http, server_id).await {
                    Ok(()) => (),
                    Err(err) => {
                        println!("sync roles error {} {:?}", server_id, err);
                    }
                }
            }
            println!("sync complete");
            return
        }
        Some("check_over_paired") => {
            println!("running over paired check");
            let res = verification::check_over_paired_discord_ids(&pool).await;
            match res {
                Ok(removed) => {
                    if !removed.is_empty() {
                        dbg!(removed);
                    }
                }
                Err(err) => { dbg!(err); }
            }
            return
        }
        None => (),
        _ => {
            // panic!("unknown cmd {:?}", cmd);
            println!("unknown cmd {:?}", cmd);
            return
        }
    }
    
    poise::Framework::build()
        // .prefix(">>'")
        .token(token)
        .user_data_setup(move |_ctx, _ready, _framework| {
            Box::pin(async move {
                Ok(Data {
                    pool,
                    config,
                    guide_text,
                })
            })
        })
        .options(poise::FrameworkOptions {
            // configure framework here
            on_error: |err, ctx| Box::pin(error_handler(err, ctx)),
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some(">>'".into()),
                edit_tracker: Some(poise::EditTracker::for_timespan(Duration::from_secs(3600))),
                dynamic_prefix: Some(|a ,b, c| check_prefix(a, b, c).boxed()),
                ..Default::default()
            },
            owners: owners,
            ..Default::default()
        })
        .command(help(), |f| f)
        .command(dmme(), |f| f)
        .command(guide(), |f| f)
        .command(register(), |f| f)
        .command(age(), |f| f)
        .command(new_token(), |f| f)
        .command(clear_token(), |f| f)
        .command(force_token(), |f| f)
        .command(set_comment(), |f| f)
        .command(set_comment_b(), |f| f)
        .command(test_check(), |f| f)
        .command(test_verify(), |f| f)
        .command(sync_members(), |f| f)
        .command(status(), |f| f)
        .command(statusu(), |f| f)
        .command(set_role(), |f| f)
        .run().await.unwrap();
}

