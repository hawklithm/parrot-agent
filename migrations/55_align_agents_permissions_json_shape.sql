-- Migration 55: align agents.permissions JSON shape with the Rust model
--
-- `models::AgentPermissions` serializes camelCase
-- (`canCreateAgents` / `canCreateSkills` / `trustPreset` / `authorizationPolicy`)
-- but the table default was snake_case (`can_create_agents` ...), so any agent
-- row created with the default permissions failed to deserialize on read
-- (`missing field canCreateAgents`). Align the default and backfill existing
-- snake_case rows.

ALTER TABLE agents
    ALTER COLUMN permissions
        SET DEFAULT '{"canCreateAgents":false,"canCreateSkills":false,"trustPreset":"standard","authorizationPolicy":"manual"}'::jsonb;

UPDATE agents
SET permissions = jsonb_build_object(
    'canCreateAgents',
        COALESCE((permissions->>'can_create_agents')::boolean, (permissions->>'canCreateAgents')::boolean, false),
    'canCreateSkills',
        COALESCE((permissions->>'can_create_skills')::boolean, (permissions->>'canCreateSkills')::boolean, false),
    'trustPreset',
        COALESCE(permissions->>'trust_preset', permissions->>'trustPreset', 'standard'),
    'authorizationPolicy',
        COALESCE(permissions->>'authorization_policy', permissions->>'authorizationPolicy', 'manual')
)
WHERE permissions ? 'can_create_agents'
   OR permissions ? 'can_create_skills'
   OR permissions ? 'trust_preset'
   OR permissions ? 'authorization_policy';
