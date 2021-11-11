
use rand::Rng;

pub fn to_i(x: u64) -> i64 {
    i64::from_be_bytes(x.to_be_bytes())
}

pub fn from_i(x: i64) -> u64 {
    u64::from_be_bytes(x.to_be_bytes())
}

// const TOKEN_CHARS: Vec<char> = {
//     // &['a'..'z', 'A'..'Z']
//     let mut acc = Vec::new();
//     acc.extend('a'..'z');
    
//     // &['a']
//     acc
// };

pub fn gen_token() -> String {
    let mut acc = Vec::new();
    acc.extend('a'..'z');
    acc.extend('A'..'Z');
    let slice = rand::distributions::Slice::new(&acc).unwrap();
    
    let rng = rand::thread_rng();
    let token: String = rng
        .sample_iter(&slice)
        .take(32)
        .collect();
    
    token
}

const UUID_CONTEXT: uuid::v1::Context = uuid::v1::Context::new(0);

lazy_static::lazy_static!{
    static ref UUID_TIME_BASE: std::time::Instant = {
        std::time::Instant::now()
    };
}

pub fn gen_uuid() -> String {
    // let ctx = uuid::v1::Context::new(0);
    let dur = UUID_TIME_BASE.elapsed();
    
    let ts = uuid::v1::Timestamp::from_unix(&UUID_CONTEXT, dur.as_secs(), dur.subsec_nanos());
    
    let id = uuid::Uuid::new_v1(ts, &[0, 0, 0, 0, 0, 0]).expect("uuid");
    
    let id_str = format!("{}", id.to_simple());
    
    id_str
}
