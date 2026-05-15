diesel::table! {
    fcm_credentials (id) {
        id -> Integer,
        guild_id -> Text,
        gcm_android_id -> Text,
        gcm_security_token -> Text,
        steam_id -> Text,
        issued_date -> BigInt,
        expire_date -> BigInt,
    }
}

diesel::table! {
    guild_configs (guild_id) {
        guild_id -> Text,
        setup_mode -> Text,
        manual_dashboard_channel_id -> Nullable<Text>,
        manual_chat_channel_id -> Nullable<Text>,
        manual_alerts_channel_id -> Nullable<Text>,
        manual_cctv_channel_id -> Nullable<Text>,
        in_game_prefix -> Text,
        management_channel_id -> Nullable<Text>,
    }
}

diesel::table! {
    paired_servers (id) {
        id -> Integer,
        fcm_credential_id -> Integer,
        server_ip -> Text,
        server_port -> Integer,
        player_token -> Integer,
        name -> Text,
        auto_reconnect -> Integer,
    }
}

diesel::table! {
    pairing_requests (id) {
        id -> Text,
        guild_id -> Text,
        fcm_credential_id -> Integer,
        server_ip -> Text,
        server_port -> Integer,
        player_token -> Integer,
        name -> Text,
    }
}

diesel::table! {
    player_stats (id) {
        id -> Integer,
        server_id -> Integer,
        steam_id -> Text,
        event_type -> Text,
        x -> Float,
        y -> Float,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    server_channels (server_id) {
        server_id -> Integer,
        category_id -> Nullable<Text>,
        dashboard_channel_id -> Nullable<Text>,
        chat_channel_id -> Nullable<Text>,
        alerts_channel_id -> Nullable<Text>,
        dashboard_message_id -> Nullable<Text>,
        config_channel_id -> Nullable<Text>,
        config_message_id -> Nullable<Text>,
        cctv_channel_id -> Nullable<Text>,
        cctv_message_id -> Nullable<Text>,
    }
}

diesel::table! {
    server_settings (server_id) {
        server_id -> Integer,
        in_game_prefix -> Text,
        bridge_rust_to_discord -> Integer,
        bridge_discord_to_rust -> Integer,
        command_cooldown -> Integer,
        chat_cooldown -> Integer,
        events_cargo -> Integer,
        events_heli -> Integer,
        events_oilrig -> Integer,
        events_ch47 -> Integer,
        events_vending -> Integer,
    }
}

diesel::table! {
    sessions (token) {
        token -> Text,
        discord_id -> Text,
        expires_at -> Timestamp,
    }
}

diesel::table! {
    user_rustplus_credentials (discord_id) {
        discord_id -> Text,
        gcm_android_id -> Text,
        gcm_security_token -> Text,
        expo_push_token -> Text,
        rustplus_auth_token -> Text,
    }
}

diesel::table! {
    users (discord_id) {
        discord_id -> Text,
        username -> Text,
        avatar -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::joinable!(fcm_credentials -> guild_configs (guild_id));
diesel::joinable!(paired_servers -> fcm_credentials (fcm_credential_id));
diesel::joinable!(pairing_requests -> fcm_credentials (fcm_credential_id));
diesel::joinable!(pairing_requests -> guild_configs (guild_id));
diesel::joinable!(player_stats -> paired_servers (server_id));
diesel::joinable!(server_channels -> paired_servers (server_id));
diesel::joinable!(server_settings -> paired_servers (server_id));

diesel::joinable!(sessions -> users (discord_id));
diesel::joinable!(user_rustplus_credentials -> users (discord_id));

diesel::allow_tables_to_appear_in_same_query!(
    fcm_credentials,
    guild_configs,
    paired_servers,
    pairing_requests,
    player_stats,
    server_channels,
    server_settings,
    sessions,
    user_rustplus_credentials,
    users,
);
