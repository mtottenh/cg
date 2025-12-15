-- Sub-Phase 3.3: Availability System
-- Allows players/teams to specify weekly availability windows

-- Availability windows (recurring weekly slots)
CREATE TABLE availability_windows (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Who this availability belongs to (player or team)
    -- One of these must be set
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    registration_id UUID REFERENCES tournament_registrations(id) ON DELETE CASCADE,

    -- Day of week (0 = Sunday, 1 = Monday, ... 6 = Saturday)
    day_of_week SMALLINT NOT NULL CHECK (day_of_week >= 0 AND day_of_week <= 6),

    -- Time range (in UTC)
    start_time TIME NOT NULL,
    end_time TIME NOT NULL,

    -- Optional timezone preference for display
    timezone VARCHAR(64),

    -- Whether this is a preference or hard constraint
    is_preferred BOOLEAN NOT NULL DEFAULT true,

    -- Optional notes
    notes TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensure at least one owner is set
    CONSTRAINT availability_has_owner CHECK (
        player_id IS NOT NULL OR registration_id IS NOT NULL
    ),

    -- Ensure start time is before end time
    CONSTRAINT valid_time_range CHECK (start_time < end_time)
);

-- Index for efficient lookups by player or registration
CREATE INDEX idx_availability_windows_player_id ON availability_windows(player_id) WHERE player_id IS NOT NULL;
CREATE INDEX idx_availability_windows_registration_id ON availability_windows(registration_id) WHERE registration_id IS NOT NULL;

-- Unique constraint to prevent duplicate slots
CREATE UNIQUE INDEX idx_availability_unique_slot_player ON availability_windows(player_id, day_of_week, start_time, end_time)
    WHERE player_id IS NOT NULL;
CREATE UNIQUE INDEX idx_availability_unique_slot_registration ON availability_windows(registration_id, day_of_week, start_time, end_time)
    WHERE registration_id IS NOT NULL;

-- One-time date overrides (blocked dates or specific availability)
CREATE TABLE availability_overrides (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Who this override belongs to
    player_id UUID REFERENCES players(id) ON DELETE CASCADE,
    registration_id UUID REFERENCES tournament_registrations(id) ON DELETE CASCADE,

    -- Specific date (not recurring)
    override_date DATE NOT NULL,

    -- Time range (null = all day)
    start_time TIME,
    end_time TIME,

    -- Type: 'blocked' (unavailable) or 'available' (extra availability)
    override_type VARCHAR(16) NOT NULL CHECK (override_type IN ('blocked', 'available')),

    -- Reason for override
    reason TEXT,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Ensure at least one owner is set
    CONSTRAINT override_has_owner CHECK (
        player_id IS NOT NULL OR registration_id IS NOT NULL
    ),

    -- Ensure if times are provided, they are valid
    CONSTRAINT valid_override_time_range CHECK (
        (start_time IS NULL AND end_time IS NULL) OR
        (start_time IS NOT NULL AND end_time IS NOT NULL AND start_time < end_time)
    )
);

-- Index for efficient lookups
CREATE INDEX idx_availability_overrides_player_id ON availability_overrides(player_id) WHERE player_id IS NOT NULL;
CREATE INDEX idx_availability_overrides_registration_id ON availability_overrides(registration_id) WHERE registration_id IS NOT NULL;
CREATE INDEX idx_availability_overrides_date ON availability_overrides(override_date);

-- Suggested meeting times based on availability overlap
CREATE TABLE suggested_times (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Match this suggestion is for
    match_id UUID NOT NULL REFERENCES tournament_matches(id) ON DELETE CASCADE,

    -- Suggested time slot
    suggested_start TIMESTAMPTZ NOT NULL,
    suggested_end TIMESTAMPTZ NOT NULL,

    -- Confidence score (0-100) based on overlap quality
    confidence_score INTEGER NOT NULL DEFAULT 50 CHECK (confidence_score >= 0 AND confidence_score <= 100),

    -- Whether this slot overlaps with both participants' availability
    is_mutual_overlap BOOLEAN NOT NULL DEFAULT false,

    -- Whether this was auto-generated or manually suggested
    is_auto_generated BOOLEAN NOT NULL DEFAULT true,

    -- Status of the suggestion
    status VARCHAR(32) NOT NULL DEFAULT 'suggested' CHECK (
        status IN ('suggested', 'accepted', 'rejected', 'expired')
    ),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT valid_suggestion_range CHECK (suggested_start < suggested_end)
);

CREATE INDEX idx_suggested_times_match_id ON suggested_times(match_id);
CREATE INDEX idx_suggested_times_status ON suggested_times(status);

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_availability_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tr_availability_windows_updated_at
    BEFORE UPDATE ON availability_windows
    FOR EACH ROW
    EXECUTE FUNCTION update_availability_updated_at();

CREATE TRIGGER tr_availability_overrides_updated_at
    BEFORE UPDATE ON availability_overrides
    FOR EACH ROW
    EXECUTE FUNCTION update_availability_updated_at();

CREATE TRIGGER tr_suggested_times_updated_at
    BEFORE UPDATE ON suggested_times
    FOR EACH ROW
    EXECUTE FUNCTION update_availability_updated_at();

-- Comments
COMMENT ON TABLE availability_windows IS 'Recurring weekly availability slots for players or tournament registrations';
COMMENT ON TABLE availability_overrides IS 'One-time date overrides (blocked or extra availability)';
COMMENT ON TABLE suggested_times IS 'Auto-generated or manual time suggestions for match scheduling';
