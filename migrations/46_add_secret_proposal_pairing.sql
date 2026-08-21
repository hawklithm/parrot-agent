-- Allow a binding proposal to depend on a pending secret proposal.
ALTER TABLE company_secret_proposals
    ADD COLUMN IF NOT EXISTS secret_proposal_id UUID
        REFERENCES company_secret_proposals(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_secret_proposals_secret_proposal_id
    ON company_secret_proposals(secret_proposal_id);
