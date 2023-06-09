CREATE TABLE oauth_authorization (
    id INTEGER GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    code VARCHAR(100) UNIQUE NOT NULL,
    user_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    application_id INTEGER NOT NULL REFERENCES oauth_application (id) ON DELETE CASCADE,
    scopes VARCHAR(200) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL
);
