CREATE TABLE pinstar_diagrams (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    diagram_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    format TEXT NOT NULL DEFAULT 'canvas'
);

CREATE INDEX idx_pinstar_diagrams_owner_id ON pinstar_diagrams (owner_id);

CREATE TABLE pinstar_diagram_members (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    diagram_id UUID NOT NULL REFERENCES pinstar_diagrams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',
    UNIQUE (diagram_id, user_id)
);

CREATE INDEX idx_pinstar_diagram_members_user_id ON pinstar_diagram_members (user_id);
CREATE INDEX idx_pinstar_diagram_members_diagram_id ON pinstar_diagram_members (diagram_id);

CREATE TABLE pinstar_invites (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    diagram_id UUID NOT NULL REFERENCES pinstar_diagrams(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL DEFAULT 'editor',
    uses_left INT,
    expires_at TIMESTAMPTZ
);

CREATE INDEX idx_pinstar_invites_token ON pinstar_invites (token);
CREATE INDEX idx_pinstar_invites_diagram_id ON pinstar_invites (diagram_id);
