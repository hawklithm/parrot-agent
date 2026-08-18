-- Add missing lock-related fields to documents table
-- These fields were present in the original schema but removed in migration 12

ALTER TABLE documents 
ADD COLUMN IF NOT EXISTS locked_by_type TEXT,
ADD COLUMN IF NOT EXISTS locked_by_id UUID,
ADD COLUMN IF NOT EXISTS locked_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS locked_run_id UUID;

-- Create index for lock queries
CREATE INDEX IF NOT EXISTS idx_documents_locked_by ON documents(locked_by_type, locked_by_id);
CREATE INDEX IF NOT EXISTS idx_documents_locked_at ON documents(locked_at);
