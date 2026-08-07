-- Add 'planned' status to project_status enum
-- This aligns with paperclip's project status values

ALTER TYPE project_status ADD VALUE 'planned' BEFORE 'backlog';
