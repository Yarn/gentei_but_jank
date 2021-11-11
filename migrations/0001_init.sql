
create schema genteib;

create table genteib.servers (
    server_id bigint NOT NULL,
    default_yt_channel_id text,
    -- {yt_channel_id(int): role_id(string)}
    "roles" jsonb NOT NULL DEFAULT '{}',
  	PRIMARY KEY ("server_id")
);

create table genteib.users (
    discord_id bigint NOT NULL,
    -- server_id bigint NOT NULL,
    yt_channel_id text NOT NULL,
    yt_channel_n bigint NOT NULL DEFAULT 0,
    token text NOT NULL,
    last_verified timestamp DEFAULT NULL,
    last_channel_verified timestamp DEFAULT NULL,
    last_checked timestamp DEFAULT NULL,
    failed_checks bigint NOT NULL DEFAULT 0,
    yt_video_id text DEFAULT NULL,
    yt_comment_id text DEFAULT NULL,
    user_yt_channel_id text DEFAULT NULL,
    extra jsonb NOT NULL DEFAULT '{}',
    -- PRIMARY KEY (discord_id, server_id, yt_channel_id)
    PRIMARY KEY (discord_id, yt_channel_id, yt_channel_n)
);
