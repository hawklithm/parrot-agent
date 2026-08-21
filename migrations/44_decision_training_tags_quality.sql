-- Persist the reusable decision-training labels that the service contract exposes.
ALTER TABLE decision_training_examples
    ADD COLUMN IF NOT EXISTS tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS quality_score REAL;

ALTER TABLE decision_training_examples
    DROP CONSTRAINT IF EXISTS decision_training_examples_tags_array_check;

ALTER TABLE decision_training_examples
    ADD CONSTRAINT decision_training_examples_tags_array_check
    CHECK (jsonb_typeof(tags) = 'array');

ALTER TABLE decision_training_examples
    DROP CONSTRAINT IF EXISTS decision_training_examples_quality_score_check;

ALTER TABLE decision_training_examples
    ADD CONSTRAINT decision_training_examples_quality_score_check
    CHECK (quality_score IS NULL OR (quality_score >= 0 AND quality_score <= 1));
