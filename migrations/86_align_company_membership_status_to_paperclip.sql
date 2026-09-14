-- Align company membership lifecycle states with the Paperclip access API.
--
-- The original Parrot schema only had active/inactive.  The web client
-- exposes pending/active/suspended/archived, so keep the legacy value for
-- older repository code while adding the complete API vocabulary. Existing
-- inactive memberships are the same soft-deleted state as archived.

ALTER TYPE company_membership_status ADD VALUE IF NOT EXISTS 'pending';
ALTER TYPE company_membership_status ADD VALUE IF NOT EXISTS 'suspended';
ALTER TYPE company_membership_status ADD VALUE IF NOT EXISTS 'archived';
