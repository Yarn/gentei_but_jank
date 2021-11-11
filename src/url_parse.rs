
use url::{ Url, Host };

pub fn is_url(maybe_url: &str) -> bool {
    maybe_url.contains(":") && maybe_url.contains("/")
}

pub fn extract_video_comment_id(video_url: &str) -> Option<(String, Option<String>)> {
    let url = Url::parse(video_url).ok()?;
    
    let host = url.host()?;
    let host = match host {
        Host::Domain(host) => host,
        _ => return None,
    };
    
    if host == "youtu.be" {
        let mut segs = url.path_segments()?;
        let res = segs.next()?;
        if segs.next().is_some() {
            return None
        }
        return Some((res.into(), None));
    }
    
    if !&["www.youtube.com", "youtube.com"].contains(&host) {
        return None
    }
    
    let mut segs = url.path_segments()?;
    let seg = segs.next()?;
    
    match seg {
        "embed" => {
            let res = segs.next()?;
            if segs.next().is_some() {
                return None
            }
            Some((res.into(), None))
        }
        "watch" => {
            if segs.next().is_some() {
                return None
            }
            let mut video_id = None;
            let mut comment_id = None;
            for (k, v) in url.query_pairs() {
                if k == "v" {
                    video_id = Some(v.into())
                } else if k == "lc" {
                    comment_id = Some(v.into())
                }
            }
            return video_id.map(|v| (v, comment_id))
        }
        _ => None
    }
}

pub fn extract_channel_id(channel_url: &str) -> Option<String> {
    let url = Url::parse(channel_url).ok()?;
    
    let host = url.host()?;
    let host = match host {
        Host::Domain(host) => host,
        _ => return None,
    };
    
    if !&["www.youtube.com", "youtube.com"].contains(&host) {
        return None
    }
    
    let mut segs = url.path_segments()?;
    
    if segs.next()? != "channel" {
        return None
    }
    
    segs.next().map(|s| s.into())
}
