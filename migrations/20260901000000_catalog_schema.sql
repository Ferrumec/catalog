-- Catalog & Discovery Service schema (catalog_db).
--
-- Adapted from SDD-2026-001 v1.1 §7.2 with one correction: the SDD's SQL
-- has `owner_id UUID NOT NULL REFERENCES users(id)`, a foreign key into
-- the Identity service's `users` table. Per DES-006 (database-per-service)
-- catalog_db and identity_db are separate Postgres databases/instances,
-- so a real FK constraint across them isn't possible — Postgres can't
-- enforce a cross-database reference. `owner_id` (and the other *_by
-- columns below) are kept as plain UUID columns instead: the value comes
-- from the caller's authenticated identity (see src/identity.rs) and is
-- trusted at the boundary rather than enforced at the DB layer, which is
-- the normal shape for database-per-service designs.

CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    slug VARCHAR(255) UNIQUE NOT NULL,
    description TEXT,
    documentation_url VARCHAR(500),
    owner_id UUID NOT NULL,
    current_version_id UUID,
    is_published BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMP WITH TIME ZONE,
    popularity_score DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    search_vector TSVECTOR,
    CONSTRAINT check_name_length CHECK (char_length(name) >= 3)
);
CREATE INDEX idx_projects_owner_id ON projects(owner_id);
CREATE INDEX idx_projects_slug ON projects(slug);
CREATE INDEX idx_projects_search ON projects USING GIN(search_vector);
-- Soft-deleted / unpublished projects are excluded from almost every
-- query (discovery, get-by-slug, etc.); a partial index keeps those
-- lookups on the common path cheap.
CREATE INDEX idx_projects_visible ON projects(id) WHERE deleted_at IS NULL AND is_published = TRUE;

-- PRJ-016/DSC-004: metadata includes "tags and categories". The SDD's
-- schema only modeled tags (free-form, many-to-many). Categories are
-- added here as a small controlled list, many-to-many like tags but
-- distinct so discovery can filter/facet on them independently
-- (DSC-004 asks for both, and conflating them would make "filter by
-- category" and "filter by tag" the same query, which isn't what a
-- controlled taxonomy vs. free-form tagging usually means in practice).
CREATE TABLE project_tags (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    tag VARCHAR(50) NOT NULL,
    PRIMARY KEY (project_id, tag)
);
CREATE INDEX idx_project_tags_tag ON project_tags(tag);

CREATE TABLE project_categories (
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    category VARCHAR(50) NOT NULL,
    PRIMARY KEY (project_id, category)
);
CREATE INDEX idx_project_categories_category ON project_categories(category);

-- PRJ-018: metadata version history for audit purposes. Kept local and
-- append-only here (fast reads for "who changed what, when" on a single
-- project) in addition to whatever the Admin & Audit Service's
-- cross-service audit_logs table separately records off the published
-- events (Section 8.3 / 7.5) — this table is the catalog's own detail
-- record, not a replacement for that audit trail.
CREATE TABLE project_metadata_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    changed_by UUID NOT NULL,
    changed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    previous_value JSONB NOT NULL,
    new_value JSONB NOT NULL
);
CREATE INDEX idx_metadata_history_project_id ON project_metadata_history(project_id);

CREATE TABLE versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    version_tag VARCHAR(50) NOT NULL,
    artifact_url VARCHAR(500) NOT NULL,
    artifact_size BIGINT NOT NULL,
    checksum VARCHAR(128) NOT NULL,
    is_yanked BOOLEAN NOT NULL DEFAULT FALSE,
    yank_reason TEXT,
    yanked_at TIMESTAMP WITH TIME ZONE,
    yanked_by UUID,
    unyanked_at TIMESTAMP WITH TIME ZONE,
    unyanked_by UUID,
    validation_status VARCHAR(20) NOT NULL DEFAULT 'pending',
    validation_details JSONB,
    developer_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    validated_at TIMESTAMP WITH TIME ZONE,
    UNIQUE(project_id, version_tag),
    CONSTRAINT check_validation_status CHECK (
        validation_status IN ('pending', 'validating', 'passed', 'failed')
    )
);
CREATE INDEX idx_versions_project_id ON versions(project_id);
CREATE INDEX idx_versions_is_yanked ON versions(is_yanked) WHERE is_yanked = TRUE;
CREATE INDEX idx_versions_validation_status ON versions(validation_status);

ALTER TABLE projects
    ADD CONSTRAINT fk_projects_current_version
    FOREIGN KEY (current_version_id) REFERENCES versions(id);

-- Keep search_vector in sync on write rather than computing it in every
-- query. name is weighted higher than description so a name match
-- ranks above an incidental description match.
CREATE FUNCTION projects_search_vector_update() RETURNS trigger AS $$
BEGIN
    NEW.search_vector :=
        setweight(to_tsvector('english', coalesce(NEW.name, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(NEW.description, '')), 'B');
    RETURN NEW;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_projects_search_vector
    BEFORE INSERT OR UPDATE OF name, description ON projects
    FOR EACH ROW EXECUTE FUNCTION projects_search_vector_update();
