-- Widen companies.issue_prefix to TEXT.
--
-- The prefix allocator appends one 'A' per retry attempt
-- (see CompanyRepository::suffix_for_attempt), so the 9th company sharing a
-- 3-letter base derives an 11-character prefix and the insert failed with
-- SQLSTATE 22001 ("value too long for type character varying(10)"). That error
-- was not recognized as a prefix conflict, so the insert propagated and company
-- creation returned HTTP 500 instead of advancing to the next candidate.
--
-- Paperclip stores the same column as an unbounded text column, so widening
-- restores parity and lets the allocator's retry ladder run to completion.

ALTER TABLE companies ALTER COLUMN issue_prefix TYPE TEXT;
