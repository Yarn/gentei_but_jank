
use tokio::process::Command;
use std::process::Stdio;
use anyhow::Result;
use anyhow::anyhow;
use lazy_static::lazy_static;

use governor::{
    RateLimiter, Quota,
    state::{ direct:: NotKeyed, InMemoryState },
    clock::DefaultClock,
};

use crate::GOOJF;

lazy_static! {
    pub static ref RATE_LIMIT: RateLimiter<NotKeyed, InMemoryState, DefaultClock> = {
        RateLimiter::direct(Quota::per_second(std::num::NonZeroU32::new(2).unwrap()))
    };
    
    static ref CHECK_PROGRAM: String = {
        std::env::var("check_program")
            .unwrap_or_else(|_| "python".into())
    };
    
    static ref CHECK_ARGS: Vec<String> = {
        let raw = std::env::var("check_args")
            .unwrap_or_else(|_| "./comment_scrapper/downloader.py".into());
        raw.split("  ").map(|s| s.to_string()).collect()
    };
}
// const rate_limit: RateLimiter<NotKeyed, InMemoryState, DefaultClock> =
//     RateLimiter::direct(Quota::per_second(std::num::NonZeroU32::new(2).unwrap()));

pub enum MembershipStatus {
    /// Is a member
    Member {
        channel_id: String,
        user_channel_id: String,
        text: String,
    },
    /// Is not a member
    Not {
        channel_id: String,
        user_channel_id: String,
        text: String,
    },
    /// comment not found
    NotFound,
}
pub use MembershipStatus::{Member, Not, NotFound};

pub struct VideoInfo {
    pub channel_name: String,
    #[allow(dead_code)]
    channel_id: String,
}

fn check_id(id: &str) -> bool {
    // '.' appears in comment ids when they are a reply
    id.chars().all(|c| ('a'..='z').contains(&c) || ('A'..='Z').contains(&c) || ('0'..='9').contains(&c) || c == '-' || c == '_' || c == '.')
}

#[derive(serde::Deserialize)]
struct ResultData {
    is_member: bool,
    #[serde(rename = "channel_id")]
    channel: String,
    /// channel of user that made the comment
    #[serde(rename = "channel")]
    user_channel: String,
    text: String,
}

#[derive(serde::Deserialize)]
struct VideoResultData {
    channel_id: String,
    channel_name: String,
}

pub async fn check_member(video_id: &str, comment_id: &str) -> Result<(VideoInfo, MembershipStatus)> {
    RATE_LIMIT.until_ready().await;
    
    // let python_path = std::env::var("check_program").unwrap();
    // let script_path = std::env::var("check_args").unwrap();
    let python_path: &str = &CHECK_PROGRAM;
    // let script_path = 
    
    if !check_id(video_id) {
        return Err(anyhow!("invalid channel id"));
        // return Ok(Not("invalid channel id"));
    }
    if !check_id(comment_id) {
        return Err(anyhow!("invalid comment id"));
        // return Ok(Not("invalid comment id"));
    }
    
    let id_arg = format!("{}&lc={}", video_id, comment_id);
    
    let mut cmd = Command::new(python_path);
    // cmd.arg(script_path)
    for arg in CHECK_ARGS.iter() {
        cmd.arg(&*arg);
    }
    cmd
        .arg("--youtubeid")
        .arg(&id_arg)
        .arg("-s").arg("0")
        .arg("-l").arg("1")
        .arg("--goojf").arg(&*GOOJF);
    
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    
    let output = child.wait_with_output().await?;
    
    if !output.status.success() {
        return Err(anyhow!("child exited with exit status {} {}\n{:?}", output.status, id_arg, output))
    }
    
    let out_str = std::str::from_utf8(&output.stdout)?;
    let mut split = out_str.split('\n');
    
    let video_info_str = split.next().ok_or_else(|| anyhow!("no channel info returned by scraper"))?;
    let comment_info_str = split.next().unwrap_or("");
    
    let video_data: VideoResultData = serde_json::from_str(video_info_str)?;
    
    let video_info = VideoInfo {
        channel_id: video_data.channel_id,
        channel_name: video_data.channel_name,
    };
    
    if comment_info_str.is_empty() {
        return Ok((video_info, NotFound))
    }
    
    // dbg!(&output);
    let data: ResultData = serde_json::from_str(comment_info_str)
        .map_err(|e| {dbg!(&output); e})?;
    
    if !data.is_member {
        return Ok((video_info, Not{ channel_id: data.channel, user_channel_id: data.user_channel, text: data.text }))
    }
    
    Ok((video_info, Member{ channel_id: data.channel, user_channel_id: data.user_channel, text: data.text }))
}
