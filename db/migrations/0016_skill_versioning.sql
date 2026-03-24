-- SKL-012: Add version pinning to skill_packs
-- SKL-013: Add deprecation flag to skill_packs
-- SKL-005: Add expected_output_contract to skill_packs

ALTER TABLE skill_packs
    ADD COLUMN IF NOT EXISTS version TEXT,
    ADD COLUMN IF NOT EXISTS deprecated BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS expected_output_contract TEXT;

-- SKL-011: Add project_default_skill_pack to user_policies
-- This is stored inside the policy_payload JSONB as
-- policy_payload->'global'->'default_skill_pack_id'.
-- No schema change needed since user_policies.policy_payload is already JSONB.
-- This comment documents the convention for SKL-011.

-- SKL-012: version CHECK -- allow semver-style or NULL
DO $$ BEGIN
    ALTER TABLE skill_packs ADD CONSTRAINT chk_skill_packs_version
        CHECK (version IS NULL OR version ~ '^[0-9]+\.[0-9]+\.[0-9]+([a-zA-Z0-9._+-]*)?$');
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
