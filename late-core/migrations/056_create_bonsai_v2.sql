CREATE TABLE bonsai_v2_trees (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    updated TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE UNIQUE,
    seed BIGINT NOT NULL,
    last_watered DATE,
    is_alive BOOLEAN NOT NULL DEFAULT true,
    vigor INT NOT NULL DEFAULT 70,
    water_stress INT NOT NULL DEFAULT 0,
    last_simulated_date DATE NOT NULL DEFAULT current_date,
    branch_graph JSONB NOT NULL,
    selected_branch_id INT,
    mode TEXT NOT NULL DEFAULT 'inspect',
    badge_glyph TEXT NOT NULL DEFAULT '·',
    planted_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    state_revision BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX idx_bonsai_v2_trees_user_updated
    ON bonsai_v2_trees(user_id, updated DESC);
