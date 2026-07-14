ALTER TABLE broker_profiles ADD COLUMN protocol TEXT NOT NULL DEFAULT 'mqtt';
ALTER TABLE broker_profiles ADD COLUMN websocket_path TEXT;
