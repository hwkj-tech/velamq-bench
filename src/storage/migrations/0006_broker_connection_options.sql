ALTER TABLE broker_profiles ADD COLUMN mqtt_version TEXT NOT NULL DEFAULT 'v3_1_1';
ALTER TABLE broker_profiles ADD COLUMN connection_timeout_secs INTEGER NOT NULL DEFAULT 10;
ALTER TABLE broker_profiles ADD COLUMN mqtt5_json TEXT;
