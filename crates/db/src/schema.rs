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
    vending_subscriptions (id) {
        id -> Integer,
        discord_id -> Nullable<Text>,
        steam_id -> Nullable<Text>,
        server_id -> Integer,
        item_id -> Integer,
        item_name -> Text,
        max_price -> Nullable<Integer>,
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
        manual_ai_channel_id -> Nullable<Text>,
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
        ai_channel_id -> Nullable<Text>,
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
diesel::joinable!(vending_subscriptions -> paired_servers (server_id));
diesel::joinable!(sessions -> users (discord_id));
diesel::joinable!(user_rustplus_credentials -> users (discord_id));

diesel::table! {
    player_name_history (id) {
        id -> Integer,
        tracked_player_id -> Integer,
        name -> Text,
        seen_at -> Timestamp,
    }
}

diesel::table! {
    track_groups (id) {
        id -> Integer,
        server_id -> Integer,
        name -> Text,
        color -> Nullable<Text>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    track_notifications_config (id) {
        id -> Integer,
        server_id -> Integer,
        discord_channel_id -> Nullable<Text>,
        dashboard_message_id -> Nullable<Text>,
        in_game_alerts -> Integer,
        alert_on_join -> Integer,
        alert_on_leave -> Integer,
        alert_on_name_change -> Integer,
    }
}

diesel::table! {
    tracked_players (id) {
        id -> Integer,
        group_id -> Nullable<Integer>,
        server_id -> Integer,
        steam_id -> Text,
        bm_player_id -> Nullable<Text>,
        last_known_name -> Nullable<Text>,
        last_known_server_id -> Nullable<Text>,
        is_online -> Integer,
        last_seen -> Nullable<Timestamp>,
        created_at -> Timestamp,
    }
}

diesel::joinable!(player_name_history -> tracked_players (tracked_player_id));
diesel::joinable!(track_groups -> paired_servers (server_id));
diesel::joinable!(track_notifications_config -> paired_servers (server_id));
diesel::joinable!(tracked_players -> paired_servers (server_id));
diesel::joinable!(tracked_players -> track_groups (group_id));

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
    vending_subscriptions,
    player_name_history,
    track_groups,
    track_notifications_config,
    tracked_players,
);
