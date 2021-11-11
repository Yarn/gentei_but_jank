
use std::fmt;
use anyhow::anyhow;
use anyhow::Context;
use chrono::{ DateTime, Utc, TimeZone };
use chrono::naive::{NaiveDateTime};

// use serenity::futures::TryFutureExt;
use poise::serenity::http::client::Http;
// use poise::serenity::CacheAndHttp;
use poise::serenity::model::guild::Guild;
use poise::serenity::model::id::GuildId;
use poise::serenity::model::id::UserId;
use poise::serenity::model::id::RoleId;
// use sqlx::prelude::Executor;
// use sqlx::Transaction;
// use sqlx::Postgres;
use sqlx::{ PgPool };
use crate::util::{to_i, from_i};

use crate::check_wrapper::{check_member, Member, Not, NotFound};

#[derive(Debug)]
pub enum HumanContext {
    NotAMember,
    CouldNotLoadComment,
    TokenNotInComment,
    WrongChannel {
        correct: String,
        actual: String,
    },
    UserNotConfigured,
    CommentNotSet,
    TooManyFailures,
    OverPairedDiscordId,
}

impl fmt::Display for HumanContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HumanContext::NotAMember =>
                write!(f, "Not a member"),
            HumanContext::CouldNotLoadComment =>
                write!(f, "Could not load video or comment"),
            HumanContext::TokenNotInComment =>
                write!(f, "Comment does not contain token"),
            HumanContext::WrongChannel{ correct, actual } =>
                write!(f, "Comment is not on the correct channel {} != (actual){}", correct, actual),
            HumanContext::UserNotConfigured =>
                write!(f, "No token has been created for channel"),
            HumanContext::CommentNotSet =>
                write!(f, "No comment set for channel"),
            HumanContext::TooManyFailures =>
                write!(f, "Too many consecutive failures"),
            HumanContext::OverPairedDiscordId =>
                write!(f, "Too many discord ids paired to youtube account"),
        }
    }
}

pub async fn check_over_paired_discord_ids(pool: &PgPool) -> Result<Vec<String>, anyhow::Error> {
    let rows = sqlx::query!(r#"
        with counts as (
            select user_yt_channel_id, count(distinct discord_id) "n"
            from genteib.users
            where user_yt_channel_id is not null
            group by user_yt_channel_id
        )
        select user_yt_channel_id "user_yt_channel_id!"
        from counts
        where n > 3
    "#)
        .fetch_all(pool).await
        .context("select")?;
    
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    
    let user_chan_ids: Vec<_> = rows.into_iter().map(|r| r.user_yt_channel_id).collect();
    
    // delete from genteib.users where user_yt_channel_id = ANY (ARRAY['','']);
    sqlx::query!(r#"
        delete from genteib.users
        where user_yt_channel_id = ANY ($1)
    "#,
        &user_chan_ids
    )
        .execute(pool).await
        .context("delete")?;
    
    Ok(user_chan_ids)
}

pub async fn verify_pending(pool: &PgPool, n: usize) -> Result<Vec<VerifyResult>, anyhow::Error> {
    let pending: Vec<(i64, String, i64)> = sqlx::query_as(r#"
        SELECT discord_id, yt_channel_id, yt_channel_n
        FROM genteib.users
        WHERE
            yt_video_id IS NOT NULL AND
            yt_comment_id IS NOT NULL AND
            failed_checks <= 2 AND
            current_timestamp - last_checked > INTERVAL '2 days' AND
            current_timestamp - last_verified > INTERVAL '2 days'
        LIMIT $1
    "#)
        .bind(n as i32)
        .fetch_all(pool).await
        .context("get pending")?;
    
    // dbg!(&pending);
    let mut results = Vec::new();
    for (discord_id, yt_channel_id, yt_channel_n) in pending {
        let discord_id = from_i(discord_id);
        let res = update_verification(pool, discord_id, &yt_channel_id, yt_channel_n).await
            .context(format!("update_verification {} {}", discord_id, yt_channel_id))?;
        results.push(res);
    }
    
    Ok(results)
}

#[derive(Debug)]
pub struct VerifyResult {
    /// Id of discord user
    pub discord_id: u64,
    pub yt_channel_id: String,
    pub channel_name: String,
    pub was_member: bool,
    pub is_member: bool,
    pub ownership_verified: bool,
    pub errors: Vec<HumanContext>,
}

#[derive(Debug)]
pub struct UpdateRolesResult {
    pub role_errors: Vec<anyhow::Error>,
}

impl VerifyResult {
    pub fn became_member(&self) -> bool {
        !self.was_member && self.is_member
    }
    
    pub fn became_non_member(&self) -> bool {
        self.was_member && !self.is_member
    }
    
    async fn set_role(&self, http: &Http, guild_id: u64, role_id: u64) -> Result<(), anyhow::Error> {
        let guild_id = GuildId(guild_id);
        let role_id = RoleId(role_id);
        let user_id = UserId(self.discord_id);
        let guild = Guild::get(http, guild_id).await?;
        let mut member = guild.member(http, user_id).await?;
        
        if self.became_member() {
            member.add_role(http, role_id).await?;
        } else if self.became_non_member() {
            member.remove_role(http, role_id).await?;
        }
        
        Ok(())
    }
    
    pub async fn update_roles(&self, pool: &PgPool, http: &Http) -> Result<Option<UpdateRolesResult>, anyhow::Error> {
        let add = self.became_member();
        let rem = self.became_non_member();
        if !(add || rem) {
            return Ok(None)
        }
        
        // jsonb_path_query(roles, '($.keyvalue() ? (@.value == "UCV1xUwfM2v2oBtT3JNvic3w")).key') #>> '{}'
        // select all roles that correspond to the given channel
        let rows: Vec<(i64, String,)> = sqlx::query_as(&format!(r#"
            SELECT
                server_id, jsonb_path_query(roles, '($.keyvalue() ? (@.value == "{}")).key') #>> '{{}}'
            FROM genteib.servers
        "#, self.yt_channel_id))
            // .bind(&format!(r#"($.keyvalue() ? (@.value == "{}")).key"#, self.yt_channel_id))
            .fetch_all(pool).await
            .context("get roles")?;
        
        // dbg!(&rows);
        
        let mut errors = Vec::new();
        for (guild_id, role_id) in rows {
            let role_id: u64 = role_id.parse()?;
            match self.set_role(http, from_i(guild_id), role_id).await {
                Ok(()) => (),
                Err(err) => {
                    // dbg!(&err);
                    errors.push(err);
                }
            }
        }
        
        Ok(Some(UpdateRolesResult {
            role_errors: errors,
        }))
    }
}

pub async fn update_verification<'c>(
    // exec: &mut Transaction<'c, Postgres>,
    exec: &PgPool,
    user: u64, yt_channel_id: &str, yt_channel_n: i64,
    // video_id: &str, comment_id: &str
) -> Result<VerifyResult, anyhow::Error>
    // where
    //     E: Executor<'c, Database = Postgres>
{
    let row: Option<(String, Option<String>, Option<String>, i64, Option<bool>)> = sqlx::query_as(r#"
        SELECT "token", yt_video_id, yt_comment_id, failed_checks, (extra->'member_on_last_update')::bool
        FROM genteib.users
        WHERE
            discord_id = $1 AND
            yt_channel_id = $2 AND
            yt_channel_n = $3
            -- yt_video_id = $2 AND
            -- comment_id = $3
    "#)
        .bind(to_i(user))
        .bind(yt_channel_id)
        .bind(yt_channel_n)
        .fetch_optional(&*exec).await
        .context("select")?;
    
    let (token, video_id, comment_id, failed_checks, member_on_last_update) = row.ok_or_else(||
        anyhow!(
            "could not find user {}({}) {}",
            user, to_i(user), yt_channel_id,
        )
            .context(HumanContext::UserNotConfigured)
    )?;
    
    let member_on_last_update = member_on_last_update.unwrap_or(false);
    
    let (video_id, comment_id) = match (video_id, comment_id) {
        (Some(v), Some(c)) => (v, c),
        _ => {
            let err = anyhow!(
                "no comment set for user {}({}) {}",
                user, to_i(user), yt_channel_id,
            )
                .context(HumanContext::CommentNotSet);
            
            return Err(err)
        }
    };
    
    if failed_checks > 5 {
        let err = anyhow!("too many failures {}", failed_checks)
            .context(HumanContext::TooManyFailures);
        return Err(err)
    }
    
    let verify_time = Utc::now();
    
    sqlx::query(r#"
        UPDATE genteib.users
            SET
                last_checked = $4,
                failed_checks = failed_checks + 1
            WHERE
                discord_id = $1 AND
                yt_channel_id = $2 AND
                yt_channel_n = $3
    "#)
        .bind(to_i(user))
        .bind(yt_channel_id)
        .bind(yt_channel_n)
        .bind(verify_time.naive_utc())
        .execute(&*exec).await
        .context("update last checked")?;
    
    let res = check_member(&video_id, &comment_id).await?;
    
    let video_info = res.0;
    
    sqlx::query(r#"
        UPDATE genteib.users
            SET
                extra = extra || $4
            WHERE
                discord_id = $1 AND
                yt_channel_id = $2 AND
                yt_channel_n = $3
    "#)
        .bind(to_i(user))
        .bind(yt_channel_id)
        .bind(yt_channel_n)
        .bind(sqlx::types::Json(serde_json::json!({"channel_name": video_info.channel_name})))
        .execute(&*exec).await
        .context("update user channel id non member")?;
    
    let mut errors = Vec::new();
    let mut ownership_errors = Vec::new();
    
    let user_chan = match res.1 {
        Member{ channel_id: actual_channel_id, text, user_channel_id } => {
            let is_verified = if !text.contains(&token) {
                let res: Option<_> = sqlx::query!(
                    r#"
                        SELECT true
                        FROM genteib.users
                        WHERE
                            user_yt_channel_id = $1 AND
                            discord_id = $2 AND
                            --$3 - last_channel_verified < INTERVAL '2 months'
                            last_channel_verified IS NOT NULL
                    "#,
                    &user_channel_id,
                    to_i(user),
                )
                    .fetch_optional(&*exec).await
                    .context("select other verified comments")?;
                
                match res {
                    Some(_) => {
                        false
                    }
                    None => {
                        errors.push(HumanContext::TokenNotInComment);
                        false
                        
                    }
                }
            } else {
                true
            };
            
            if actual_channel_id != yt_channel_id {
                errors.push(HumanContext::WrongChannel { correct: yt_channel_id.into(), actual: actual_channel_id.clone() });
            }
            
            if is_verified {
                Some(user_channel_id)
            } else {
                None
            }
        }
        Not{ text, user_channel_id, channel_id: actual_channel_id } => {
            errors.push(HumanContext::NotAMember);
            
            if actual_channel_id != yt_channel_id {
                errors.push(HumanContext::WrongChannel { correct: yt_channel_id.into(), actual: actual_channel_id.clone() });
            }
            
            if text.contains(&token) {
                Some(user_channel_id)
            } else {
                ownership_errors.push(HumanContext::TokenNotInComment);
                None
            }
        }
        NotFound => {
            errors.push(HumanContext::CouldNotLoadComment);
            None
        }
    };
    
    let user_chan = match user_chan {
        Some(user_channel_id) => {
            // check the number of existing discord ids connected to this yt user
            let res: Option<_> = sqlx::query!(
                r#"
                    SELECT count(distinct discord_id) "n!"
                    FROM genteib.users
                    WHERE
                        user_yt_channel_id = $1 --AND
                        --discord_id = $2 AND
                        --$3 - last_channel_verified < INTERVAL '2 months'
                        --last_channel_verified IS NOT NULL
                "#,
                &user_channel_id,
                // to_i(user),
            )
                .fetch_optional(&*exec).await
                .context("select other verified comments")?;
            
            match res {
                Some(row) if row.n > 3 => {
                    ownership_errors.push(HumanContext::OverPairedDiscordId);
                    None
                }
                Some(_) | None => Some(user_channel_id),
                // None => Some(user_channel_id),
            }
            // Some(user_channel_id)
        }
        None => None,
    };
    
    let is_member = errors.is_empty();
    errors.extend(ownership_errors);
    
    let res = VerifyResult {
        discord_id: user,
        yt_channel_id: yt_channel_id.to_string(),
        channel_name: video_info.channel_name,
        was_member: member_on_last_update,
        is_member: is_member,
        ownership_verified: user_chan.is_some(),
        errors,
    };
    
    if res.is_member {
        // assert!(res.channel_verified);
        sqlx::query!(r#"
            UPDATE genteib.users
                SET
                    last_verified = $4,
                    last_channel_verified = COALESCE($6, last_channel_verified),
                    user_yt_channel_id = COALESCE($5, user_yt_channel_id),
                    failed_checks = 0,
                    extra = extra || '{"member_on_last_update": true}'
                WHERE
                    discord_id = $1 AND
                    yt_channel_id = $2 AND
                    yt_channel_n = $3
        "#,
            to_i(user),
            yt_channel_id,
            yt_channel_n,
            verify_time.naive_utc(),
            user_chan.as_deref(),
            user_chan.as_ref().map(|_| verify_time.naive_utc()),
        )
            .execute(&*exec).await
            .context("update last verified")?;
    } else {
        sqlx::query!(r#"
            UPDATE genteib.users
                SET
                    last_verified = NULL,
                    last_channel_verified = COALESCE($5, last_channel_verified),
                    user_yt_channel_id = COALESCE($4, user_yt_channel_id),
                    failed_checks = 0,
                    extra = extra || '{"member_on_last_update": false}'
                WHERE
                    discord_id = $1 AND
                    yt_channel_id = $2 AND
                    yt_channel_n = $3
        "#,
            to_i(user),
            yt_channel_id,
            yt_channel_n,
            user_chan.as_deref(),
            user_chan.as_ref().map(|_| verify_time.naive_utc()),
        )
            // .bind(sqlx::types::Json(serde_json::json!({"channel_name": video_info.channel_name})))
            .execute(&*exec).await
            .context("update user channel id non member")?;
    }
    
    Ok(res)
}

pub struct UserStatus {
    yt_channel_id: String,
    yt_channel_n: i64,
    yt_video_id: Option<String>,
    yt_comment_id: Option<String>,
    token: String,
    last_verified: Option<DateTime<Utc>>,
    last_channel_verified: Option<DateTime<Utc>>,
    last_checked: Option<DateTime<Utc>>,
    failed_checks: u64,
    is_verified: bool,
    channel_verified: bool,
    channel_name: Option<String>,
}

impl UserStatus {
    // pub fn comment_set(&self) -> bool {
    //     self.yt_video_id.is_some() && self.yt_comment_id.is_some()
    // }
    
    pub fn format_message(&self) -> String {
        let mut out = String::new();
        use std::fmt::Write;
        
        if let Some(channel_name) = self.channel_name.as_deref() {
            write!(out, "{}\n`  `", channel_name).unwrap();
        }
        
        // write!(out, "```diff\n").unwrap();
        write!(out, "<https://www.youtube.com/channel/{}>", self.yt_channel_id).unwrap();
        match self.yt_channel_n {
            0 => (),
            1 => {
                out.push_str(" '");
            }
            n => {
                write!(out, " '{}", n).unwrap();
            }
        }
        out.push('\n');
        if let (Some(vid), Some(com)) = (self.yt_video_id.as_ref(), self.yt_comment_id.as_ref()) {
            write!(out, "`  `<https://www.youtube.com/watch?v={}&lc={}>\n", vid, com).unwrap();
            write!(out, "```diff\n").unwrap();
        } else {
            write!(out, "```diff\n").unwrap();
            write!(out, "- no comment set\n").unwrap();
        }
        
        write!(out, "  token: {}\n", self.token).unwrap();
        
        if self.is_verified && self.channel_verified {
            write!(out, "+ membership verified\n").unwrap();
        } else if self.is_verified {
            write!(out, "+ membership verified (no token)\n").unwrap();
        } else if self.channel_verified {
            write!(out, "+ channel verified\n").unwrap();
        } else {
            write!(out, "- not verified\n").unwrap();
        }
        
        if self.failed_checks != 0 {
            write!(out, "- failed_checks: {}", self.failed_checks).unwrap();
        }
        
        write!(out, "```").unwrap();
        
        let mut fmt_date = |n: &str, d: &Option<DateTime<Utc>>| {
            if let Some(d) = d {
                write!(out, "`  {}:` <t:{}>\n", n, d.timestamp()).unwrap();
            } else {
                write!(out, "`  {}:` -\n", n).unwrap();
            }
        };
        
        fmt_date("verified member ", &self.last_verified);
        fmt_date("verified comment", &self.last_channel_verified);
        fmt_date("checked  fi     ", &self.last_checked);
        
        out
    }
}

pub async fn get_statuses(
    pool: &PgPool,
    user_id: u64,
) -> Result<Vec<UserStatus>, anyhow::Error> {
    let rows: Vec<(
        String, i64, Option<String>, Option<String>, String,
        Option<NaiveDateTime>, Option<NaiveDateTime>, Option<NaiveDateTime>,
        i64,
        Option<bool>,
        Option<bool>,
        Option<sqlx::types::Json<String>>,
    )> = sqlx::query_as(r#"
        SELECT
            yt_channel_id, yt_channel_n, yt_video_id, yt_comment_id, token,
            last_verified, last_channel_verified, last_checked,
            failed_checks,
            current_timestamp - last_verified < INTERVAL '3 days',
            --current_timestamp - last_channel_verified < INTERVAL '2 months',
            last_channel_verified IS NOT NULL,
            (extra->'channel_name')
        FROM genteib.users
        WHERE
            discord_id = $1
    "#)
        .bind(to_i(user_id))
        .fetch_all(pool).await
        .context("status select")?;
    
    let mut out = Vec::new();
    
    for row in rows {
        let (
            yt_channel_id, yt_channel_n, yt_video_id, yt_comment_id, token,
            last_verified, last_channel_verified, last_checked,
            failed_checks,
            is_verified,
            channel_verified,
            channel_name,
        ) = row;
        
        let failed_checks: u64 = failed_checks.try_into()?;
        let is_verified = is_verified.unwrap_or(false);
        let channel_verified = channel_verified.unwrap_or(false);
        
        let last_verified = last_verified.map(|d| Utc.from_utc_datetime(&d));
        let last_channel_verified = last_channel_verified.map(|d| Utc.from_utc_datetime(&d));
        let last_checked = last_checked.map(|d| Utc.from_utc_datetime(&d));
        
        let channel_name = channel_name.map(|x| x.0);
        
        let user_status = UserStatus {
            yt_channel_id, yt_channel_n, yt_video_id, yt_comment_id, token,
            last_verified, last_channel_verified, last_checked,
            failed_checks,
            is_verified,
            channel_verified,
            channel_name,
        };
        
        out.push(user_status);
    }
    
    Ok(out)
}
