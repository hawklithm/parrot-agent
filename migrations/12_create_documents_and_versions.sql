-- 创建 documents 和 document_versions 表

-- 先删除旧的 documents 表（如果存在）
DROP TABLE IF EXISTS annotation_comments CASCADE;
DROP TABLE IF EXISTS annotation_threads CASCADE;
DROP TABLE IF EXISTS documents CASCADE;

-- 创建新的 documents 表
CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'markdown',
    category TEXT,
    tags JSONB NOT NULL DEFAULT '[]',
    created_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    updated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    version INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'draft',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 创建 document_versions 表
CREATE TABLE IF NOT EXISTS document_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    updated_by_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    change_summary TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (document_id, version)
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_documents_company_id ON documents(company_id);
CREATE INDEX IF NOT EXISTS idx_documents_status ON documents(status);
CREATE INDEX IF NOT EXISTS idx_documents_created_at ON documents(created_at);
CREATE INDEX IF NOT EXISTS idx_document_versions_document_id ON document_versions(document_id);
CREATE INDEX IF NOT EXISTS idx_document_versions_version ON document_versions(document_id, version);

-- 重新创建 annotation_threads 和 annotation_comments（如果需要的话）
CREATE TABLE IF NOT EXISTS annotation_threads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL REFERENCES companies(id),
    document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    position JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    created_by_type TEXT,
    created_by_id UUID,
    resolved_by_type TEXT,
    resolved_by_id UUID,
    resolved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS annotation_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    thread_id UUID NOT NULL REFERENCES annotation_threads(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    actor_type TEXT NOT NULL,
    actor_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
