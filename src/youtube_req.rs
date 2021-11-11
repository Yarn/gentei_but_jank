
use std::sync::Arc;
use std::sync::RwLock;
use std::collections::HashMap;
use lazy_static::lazy_static;
use reqwest::Url;
use scraper::{ Html, Selector };
use anyhow::anyhow;
use crate::check_wrapper::RATE_LIMIT;

use crate::GOOJF;

lazy_static!{
    static ref CHANNEL_CACHE: Arc<RwLock<HashMap<String, String>>> = {
        let map = HashMap::new();
        Arc::new(RwLock::new(map))
    };
}

pub async fn get_channel_id(video_url: &str) -> Result<String, anyhow::Error> {
    {
        let cache = CHANNEL_CACHE.read()
            .map_err(|err| {
                anyhow!("could not aquire rwlock lock {:?}", err)
            })?;
        if let Some(chan_id) = cache.get(video_url) {
            return Ok(chan_id.into());
        }
    }
    
    RATE_LIMIT.until_ready().await;
    
    // let body = reqwest::get(video_url)
    //     .await?
    //     .error_for_status()?
    //     .text().await?;
    
    // let mut headers = header::HeaderMap::new();
    // headers.insert("X-MY-HEADER", header::HeaderValue::from_static("value"));
    
    let jar = reqwest::cookie::Jar::default();
    let url = "https://www.youtube.com".parse::<Url>().unwrap();
    // let url = "https://youtube.com".parse::<Url>().unwrap();
    jar.add_cookie_str(&format!("goojf={}", &*GOOJF), &url);
    jar.add_cookie_str("CONSENT=YES+cb; Domain=.youtube.com", &url);
    
    let client = reqwest::Client::builder()
        .cookie_provider(Arc::new(jar))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.130 Safari/537.36")
        .build()?;
    
    let body = client
        .get(video_url)
        // .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.130 Safari/537.36")
        .send().await?
        .error_for_status()?
        .text().await?;
    
    // println!("{}", video_url);
    // std::fs::write("yt_html_b.html", &body).expect("Unable to write file");
    
    let document = Html::parse_document(&body);
    
    let selector = Selector::parse(r#"meta[itemprop="channelId"]"#).unwrap();
    
    let elem = document.select(&selector).next().ok_or_else(|| {
        anyhow!("did not find channel id meta element in youtube response")
    })?.value();
    
    let channel_id = elem.attr("content").ok_or_else(|| {
        anyhow!("no content attribute in element")
    })?;
    
    {
        let mut cache = CHANNEL_CACHE.write()
            .map_err(|err| {
                anyhow!("could not aquire rwlock lock {:?}", err)
            })?;
        cache.insert(video_url.into(), channel_id.into());
        // if let Some(chan_id) = cache.insert(video_url, channel_id.into()) {
            // return Ok(chan_id.into());
        // }
    }
    
    Ok(channel_id.into())
}
