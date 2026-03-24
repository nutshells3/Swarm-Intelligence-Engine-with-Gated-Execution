-- Migration 0017: RMS-004, RMS-005 CHECK constraints
--
-- RMS-004: roadmap_nodes.track — constrain to valid track values.
-- RMS-005: roadmap_nodes.status — already added in 0012 with
--   ('open','deferred','rejected','absorbed','completed'). We add
--   'proposed' here so that the defer/reject schema covers all
--   lifecycle states observed in application code.
--
-- Both use idempotent DO/EXCEPTION blocks so re-running is safe.

------------------------------------------------------------------------
-- RMS-004: CHECK constraint on roadmap_nodes.track
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE roadmap_nodes ADD CONSTRAINT chk_roadmap_nodes_track
        CHECK (track IN ('active', 'deferred', 'rejected', 'proposed', 'completed', 'default'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

------------------------------------------------------------------------
-- RMS-005: Extend roadmap_nodes.status CHECK to include 'proposed'
--
-- The existing chk_roadmap_nodes_status from migration 0012 covers
-- ('open','deferred','rejected','absorbed','completed').
-- We drop and recreate to add 'proposed' as a valid status value.
------------------------------------------------------------------------
DO $$ BEGIN
    ALTER TABLE roadmap_nodes DROP CONSTRAINT IF EXISTS chk_roadmap_nodes_status;
    ALTER TABLE roadmap_nodes ADD CONSTRAINT chk_roadmap_nodes_status
        CHECK (status IN ('open', 'deferred', 'rejected', 'absorbed', 'completed', 'proposed'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;
