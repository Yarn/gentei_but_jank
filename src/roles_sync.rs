
use std::collections::{ BTreeMap, BTreeSet };
use anyhow::{ Context as _, anyhow };
use serde::{ Deserialize };
use chrono::{ Utc };

use poise::serenity::model::id::{ GuildId, RoleId };
use poise::serenity::http::Http;
use sqlx::types::Json;
use sqlx::PgPool;

// use crate::Context;
use crate::util::{ from_i, to_i };

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct Roles {
    roles: BTreeMap<String, String>,
}

pub async fn sync_roles(
    // ctx: &Context<'_>,
    pool: &PgPool,
    http: &Http,
    guild_id: u64,
) -> anyhow::Result<()> {
    let sync_time = Utc::now();
    
    let guild_id = GuildId(guild_id);
    
    // let ref http = ctx.discord().http;
    
    use poise::serenity::futures::TryStreamExt;
    // let mut guild_members = guild_id.members(http, None, None).await?;
    let mut guild_members: Vec<_> =
        guild_id.members_iter(http)
            .try_collect().await?;
    
    // println!("{:#?}", guild_members);
    
    // let mut transaction = ctx.data().pool.begin().await?;
    let mut transaction = pool.begin().await?;
    
    let res: Option<(Json<Roles>, )> = sqlx::query_as(r#"
        SELECT roles
        FROM genteib.servers
        WHERE
            server_id = $1
    "#)
        .bind(to_i(guild_id.0))
        .fetch_optional(&mut transaction).await
        .context("get server info")?;
    
    // dbg!(&res);
    let roles = match res {
        Some((Json(Roles{ roles }),)) => roles,
        None => return Err(anyhow!("server not configured {}", guild_id.0)),
    };
    
    for (role_id_str, yt_channel_id) in roles.iter() {
        let role_id = RoleId(role_id_str.parse()?);
        
        let verified: Vec<(i64,)> = sqlx::query_as(r#"
            SELECT discord_id
            FROM genteib.users
            WHERE
                yt_channel_id = $2 AND
                $1 - last_verified < INTERVAL '3 days'
        "#)
            .bind(sync_time)
            .bind(yt_channel_id)
            .fetch_all(&mut transaction).await
            .context("get verified users")?;
        let is_verified = {
            let mut set = BTreeSet::new();
            for (discord_id,) in verified {
                set.insert(from_i(discord_id));
            }
            set
        };
        
        for member in guild_members.iter_mut() {
            let verified = is_verified.contains(&member.user.id.0);
            let has_role = member.roles.contains(&role_id);
            
            if verified && !has_role {
                if let Err(err) = member.add_role(http, role_id).await {
                    println!("error adding role {:?}", err);
                };
            }
            if !verified && has_role {
                if let Err(err) = member.remove_role(http, role_id).await {
                    println!("error removing role {:?}", err);
                };
            }
        }
        
        // let has_role = {
        //     let mut set = BTreeSet::new();
        //     for member in guild_members.iter() {
        //         if member.roles.contains(&role_id) {
        //             set.insert(member.user.id);
        //         }
        //     }
        // };
    }
    
    transaction.commit().await.context("transaction commit")?;
    
    Ok(())
}
