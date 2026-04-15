ALTER TABLE guild_configs ADD COLUMN manual_cctv_channel_id TEXT;
ALTER TABLE server_channels ADD COLUMN cctv_channel_id TEXT;
ALTER TABLE server_channels ADD COLUMN cctv_message_id TEXT;