ALTER TABLE relationship ADD CONSTRAINT relationship_source_id_target_id_check CHECK (source_id != target_id);
ALTER TABLE follow_request ADD CONSTRAINT follow_request_source_id_target_id_check CHECK (source_id != target_id);
ALTER TABLE post_link ADD CONSTRAINT post_link_source_id_target_id_check CHECK (source_id != target_id);
ALTER TABLE invoice ADD CONSTRAINT invoice_sender_id_recipient_id_check CHECK (sender_id != recipient_id);
ALTER TABLE subscription ADD CONSTRAINT subscription_sender_id_recipient_id_check CHECK (sender_id != recipient_id);
